//! Token collections: every instruction, every error branch, plus the rebalance swap on live pool state.
use base64::Engine;
use litesvm::{types::FailedTransactionMetadata, types::TransactionMetadata, LiteSVM};
use sha2::{Digest, Sha256};
use solana_account::Account;
use solana_address::Address;
use solana_clock::Clock;
use solana_instruction::{account_meta::AccountMeta, Instruction};
use solana_keypair::Keypair;
use solana_message::Message;
use solana_signer::Signer;
use solana_transaction::Transaction;
use std::{collections::HashMap, path::PathBuf, str::FromStr};

const PROGRAM: &str = "CPMMoo8L3F4NbTegBCKVNunggL7H1ZpdTHKxQB5qKP1C";
const PUMP: &str = "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P";
const SYSTEM: &str = "11111111111111111111111111111111";
const TOKEN: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
const WSOL: &str = "So11111111111111111111111111111111111111112";
const RATE_ONE: u64 = 1_000_000_000;
const TRADE_FEE_RATE: u64 = 2500;

fn a(s: &str) -> Address {
    Address::from_str(s).unwrap()
}
fn disc(name: &str) -> Vec<u8> {
    Sha256::digest(name.as_bytes())[..8].to_vec()
}
fn w(k: Address) -> AccountMeta {
    AccountMeta::new(k, false)
}
fn r(k: Address) -> AccountMeta {
    AccountMeta::new_readonly(k, false)
}
fn s(k: Address) -> AccountMeta {
    AccountMeta::new(k, true)
}
fn pda(seeds: &[&[u8]], program: &Address) -> Address {
    Address::find_program_address(seeds, program).0
}
fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
}

struct Fixture {
    pool: Address,
    pump_mint: Address,
    mayhem_mint: Address,
    accounts: HashMap<Address, Account>,
}
fn fixture() -> Fixture {
    let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(root().join("svm-tests/fixtures/cpmm_wsol_pump.json")).unwrap()).unwrap();
    let mut accounts = HashMap::new();
    for (k, acc) in v["accounts"].as_object().unwrap() {
        accounts.insert(
            a(k),
            Account {
                lamports: acc["lamports"].as_u64().unwrap(),
                data: base64::engine::general_purpose::STANDARD.decode(acc["data"].as_str().unwrap()).unwrap(),
                owner: a(acc["owner"].as_str().unwrap()),
                executable: acc["executable"].as_bool().unwrap(),
                rent_epoch: 0,
            },
        );
    }
    Fixture { pool: a(v["pool"].as_str().unwrap()), pump_mint: a(v["pump_mint"].as_str().unwrap()), mayhem_mint: a(v["mayhem_mint"].as_str().unwrap()), accounts }
}

struct Pool {
    key: Address,
    amm_config: Address,
    vault0: Address,
    vault1: Address,
    mint0: Address,
    mint1: Address,
    tp0: Address,
    tp1: Address,
    observation: Address,
    dec1: u8,
    open_time: u64,
}
fn pk(d: &[u8], o: usize) -> Address {
    Address::new_from_array(d[o..o + 32].try_into().unwrap())
}

struct Env {
    svm: LiteSVM,
    program: Address,
    payer: Keypair,
    admin: Keypair,
    pool: Pool,
    pump_mint: Address,
    mayhem_mint: Address,
    user_wsol: Address,
    user_pump: Address,
}
impl Env {
    fn new() -> Self {
        let program = a(PROGRAM);
        let so = std::fs::read(root().join("target/deploy/raydium_cp_swap.so")).expect("build with --features localnet and CPSWAP_LOCALNET_ADMIN=$(solana-keygen pubkey svm-tests/fixtures/test-admin.json)");
        let admin_bytes: Vec<u8> = serde_json::from_str(&std::fs::read_to_string(root().join("svm-tests/fixtures/test-admin.json")).unwrap()).unwrap();
        let admin = Keypair::try_from(&admin_bytes[..]).unwrap();
        let mut svm = LiteSVM::new();
        svm.add_program(program, &so).unwrap();
        let f = fixture();
        for (k, acc) in &f.accounts {
            svm.set_account(*k, acc.clone()).unwrap();
        }
        let d = &f.accounts[&f.pool].data;
        let pool = Pool { key: f.pool, amm_config: pk(d, 8), vault0: pk(d, 72), vault1: pk(d, 104), mint0: pk(d, 168), mint1: pk(d, 200), tp0: pk(d, 232), tp1: pk(d, 264), observation: pk(d, 296), dec1: d[332], open_time: u64::from_le_bytes(d[373..381].try_into().unwrap()) };
        let mut clock: Clock = svm.get_sysvar();
        clock.unix_timestamp = pool.open_time as i64 + 86_400;
        svm.set_sysvar(&clock);
        let payer = Keypair::new();
        svm.airdrop(&payer.pubkey(), 1_000 * 1_000_000_000).unwrap();
        svm.airdrop(&admin.pubkey(), 10 * 1_000_000_000).unwrap();
        // User token accounts cloned from the vaults' layouts (native WSOL, Token-2022 pump) with our owner.
        let user_wsol = Address::new_unique();
        let mut wd = f.accounts[&pool.vault0].data.clone();
        wd[32..64].copy_from_slice(payer.pubkey().as_ref());
        let wsol_amt: u64 = 200 * 1_000_000_000;
        wd[64..72].copy_from_slice(&wsol_amt.to_le_bytes());
        svm.set_account(user_wsol, Account { lamports: 2_039_280 + wsol_amt, data: wd, owner: pool.tp0, executable: false, rent_epoch: 0 }).unwrap();
        let user_pump = Address::new_unique();
        let mut pd = f.accounts[&pool.vault1].data.clone();
        pd[32..64].copy_from_slice(payer.pubkey().as_ref());
        pd[64..72].copy_from_slice(&(5_000_000_000u64 * 10u64.pow(pool.dec1 as u32)).to_le_bytes());
        svm.set_account(user_pump, Account { lamports: f.accounts[&pool.vault1].lamports, data: pd, owner: pool.tp1, executable: false, rent_epoch: 0 }).unwrap();
        Env { svm, program, payer, admin, pool, pump_mint: f.pump_mint, mayhem_mint: f.mayhem_mint, user_wsol, user_pump }
    }
    fn ix(&self, keys: Vec<AccountMeta>, data: Vec<u8>) -> Instruction {
        Instruction { program_id: self.program, accounts: keys, data }
    }
    fn send(&mut self, ixs: &[Instruction], signers: &[&Keypair]) -> Result<TransactionMetadata, FailedTransactionMetadata> {
        let msg = Message::new_with_blockhash(ixs, Some(&self.payer.pubkey()), &self.svm.latest_blockhash());
        let mut all: Vec<&Keypair> = vec![&self.payer];
        for k in signers {
            if k.pubkey() != self.payer.pubkey() {
                all.push(k);
            }
        }
        let tx = Transaction::new(&all, msg, self.svm.latest_blockhash());
        let res = self.svm.send_transaction(tx);
        self.svm.expire_blockhash();
        res
    }
    #[track_caller]
    fn ok(&mut self, ixs: &[Instruction], signers: &[&Keypair]) -> TransactionMetadata {
        match self.send(ixs, signers) {
            Ok(m) => m,
            Err(e) => panic!("expected success: {:?}\n{}", e.err, e.meta.logs.join("\n")),
        }
    }
    #[track_caller]
    fn fails_with(&mut self, ixs: &[Instruction], signers: &[&Keypair], code: &str) {
        match self.send(ixs, signers) {
            Ok(m) => panic!("expected {code}, but succeeded:\n{}", m.logs.join("\n")),
            Err(e) => assert!(e.meta.logs.iter().any(|l| l.contains(&format!("Error Code: {code}"))), "expected {code}, got {:?}\n{}", e.err, e.meta.logs.join("\n")),
        }
    }
    // --- PDAs ---
    fn ruleset(&self, index: u16) -> Address {
        pda(&[b"ruleset", &index.to_le_bytes()], &self.program)
    }
    fn collection(&self, authority: &Address, index: u16) -> Address {
        pda(&[b"token_collection", authority.as_ref(), &index.to_le_bytes()], &self.program)
    }
    fn member(&self, collection: &Address, mint: &Address) -> Address {
        pda(&[b"collection_member", collection.as_ref(), mint.as_ref()], &self.program)
    }
    fn curve(&self, mint: &Address) -> Address {
        pda(&[b"bonding-curve", mint.as_ref()], &a(PUMP))
    }
    // --- instructions ---
    fn create_ruleset(&self, index: u16, kind: u8, flags: u8, program_id: Address) -> Instruction {
        let mut d = disc("global:create_ruleset");
        d.extend(index.to_le_bytes());
        d.push(kind);
        d.push(flags);
        d.extend(program_id.as_ref());
        self.ix(vec![s(self.admin.pubkey()), w(self.ruleset(index)), r(a(SYSTEM))], d)
    }
    fn update_ruleset(&self, signer: &Address, index: u16, kind: u8, flags: u8, program_id: Address) -> Instruction {
        let mut d = disc("global:update_ruleset");
        d.push(kind);
        d.push(flags);
        d.extend(program_id.as_ref());
        self.ix(vec![s(*signer), w(self.ruleset(index))], d)
    }
    fn create_collection(&self, ruleset: u16, index: u16, quote: Address, divisor: u32) -> Instruction {
        let mut d = disc("global:create_token_collection");
        d.extend(index.to_le_bytes());
        d.extend(divisor.to_le_bytes());
        self.ix(vec![s(self.payer.pubkey()), r(self.ruleset(ruleset)), r(quote), w(self.collection(&self.payer.pubkey(), index)), r(a(SYSTEM))], d)
    }
    fn update_collection(&self, signer: &Address, collection: Address, param: u8, value: u64, extra: Option<Address>) -> Instruction {
        let mut d = disc("global:update_token_collection");
        d.push(param);
        d.extend(value.to_le_bytes());
        let mut keys = vec![s(*signer), w(collection)];
        if let Some(e) = extra {
            keys.push(r(e));
        }
        self.ix(keys, d)
    }
    fn register(&self, collection: Address, ruleset: Address, mint: Address, proof: &[Address]) -> Instruction {
        let mut keys = vec![s(self.payer.pubkey()), w(collection), r(ruleset), r(mint), w(self.member(&collection, &mint)), r(a(SYSTEM))];
        keys.extend(proof.iter().map(|p| r(*p)));
        self.ix(keys, disc("global:register_collection_member"))
    }
    fn set_rate(&self, signer: &Address, collection: Address, member: Address, rate: u64) -> Instruction {
        let mut d = disc("global:set_collection_member_rate");
        d.extend(rate.to_le_bytes());
        self.ix(vec![s(*signer), r(collection), w(member)], d)
    }
    fn swap_keys(&self, zero_for_one: bool) -> Vec<AccountMeta> {
        let p = &self.pool;
        let auth = pda(&[b"vault_and_lp_mint_auth_seed"], &self.program);
        let (ua, ub, va, vb, ta, tb, ma, mb) = if zero_for_one {
            (self.user_wsol, self.user_pump, p.vault0, p.vault1, p.tp0, p.tp1, p.mint0, p.mint1)
        } else {
            (self.user_pump, self.user_wsol, p.vault1, p.vault0, p.tp1, p.tp0, p.mint1, p.mint0)
        };
        vec![s(self.payer.pubkey()), r(auth), r(p.amm_config), w(p.key), w(ua), w(ub), w(va), w(vb), r(ta), r(tb), r(ma), r(mb), w(p.observation)]
    }
    fn swap(&self, zero_for_one: bool, amount_in: u64) -> Instruction {
        let mut d = disc("global:swap_base_input");
        d.extend(amount_in.to_le_bytes());
        d.extend(0u64.to_le_bytes());
        self.ix(self.swap_keys(zero_for_one), d)
    }
    fn rebalance(&self, zero_for_one: bool, amount_in: u64, collection: Address) -> Instruction {
        let p = &self.pool;
        let (im, om) = if zero_for_one { (p.mint0, p.mint1) } else { (p.mint1, p.mint0) };
        let mut keys = self.swap_keys(zero_for_one);
        keys.extend([r(collection), r(self.member(&collection, &im)), r(self.member(&collection, &om))]);
        let mut d = disc("global:rebalance_swap_base_input");
        d.extend(amount_in.to_le_bytes());
        d.extend(0u64.to_le_bytes());
        self.ix(keys, d)
    }
    // --- helpers ---
    fn vault_amount(&self, v: &Address) -> u64 {
        u64::from_le_bytes(self.svm.get_account(v).unwrap().data[64..72].try_into().unwrap())
    }
    fn data(&self, k: &Address) -> Vec<u8> {
        self.svm.get_account(k).unwrap().data
    }
    /// LP-owned reserves: vault balance minus protocol, fund and creator fees still parked in the vault.
    fn reserves(&self) -> (u64, u64) {
        let d = self.data(&self.pool.key);
        let f = |o: usize| u64::from_le_bytes(d[o..o + 8].try_into().unwrap());
        (self.vault_amount(&self.pool.vault0) - f(341) - f(357) - f(397), self.vault_amount(&self.pool.vault1) - f(349) - f(365) - f(405))
    }
    /// Rate that values the pump reserve like the WSOL reserve (pool starts balanced).
    fn balanced_pump_rate(&self) -> u64 {
        let (v0, v1) = self.reserves();
        (v0 as u128 * 10u128.pow(self.pool.dec1 as u32) * RATE_ONE as u128 / (v1 as u128 * 10u128.pow(9))) as u64
    }
    /// A fresh SPL mint with optional authorities, plus a pump bonding curve at its PDA built from `base`.
    fn fake_mint(&mut self, mint_authority: bool, freeze_authority: bool) -> Address {
        let mint = Address::new_unique();
        let mut d = vec![0u8; 82];
        if mint_authority {
            d[0..4].copy_from_slice(&1u32.to_le_bytes());
            d[4..36].copy_from_slice(Address::new_unique().as_ref());
        }
        d[44] = 6;
        d[45] = 1;
        if freeze_authority {
            d[46..50].copy_from_slice(&1u32.to_le_bytes());
            d[50..82].copy_from_slice(Address::new_unique().as_ref());
        }
        self.svm.set_account(mint, Account { lamports: 1_461_600, data: d, owner: a(TOKEN), executable: false, rent_epoch: 0 }).unwrap();
        mint
    }
    fn fake_curve(&mut self, mint: &Address, mutate: impl Fn(&mut Vec<u8>), owner: Address) -> Address {
        let base = self.data(&self.curve(&self.pump_mint));
        let mut d = base;
        mutate(&mut d);
        let k = self.curve(mint);
        self.svm.set_account(k, Account { lamports: 2_000_000, data: d, owner, executable: false, rent_epoch: 0 }).unwrap();
        k
    }
}
fn swap_event_trade_fee(m: &TransactionMetadata) -> u64 {
    let d = disc("event:SwapEvent");
    for l in &m.logs {
        if let Some(b64) = l.strip_prefix("Program data: ") {
            let bytes = base64::engine::general_purpose::STANDARD.decode(b64).unwrap();
            if bytes.starts_with(&d) {
                return u64::from_le_bytes(bytes[153..161].try_into().unwrap());
            }
        }
    }
    panic!("no SwapEvent");
}
fn ceil_fee(amount: u64, rate: u64) -> u64 {
    ((amount as u128 * rate as u128 + 999_999) / 1_000_000) as u64
}
const KIND_ANY: u8 = 0;
const KIND_PUMP: u8 = 1;
const KIND_IMMUTABLE: u8 = 2;
const FLAG_ALLOW_MAYHEM: u8 = 1;
const FLAG_REQUIRE_COMPLETE: u8 = 2;

// ---------------------------------------------------------------- rulesets

#[test]
fn ruleset_admin_gating_and_validation() {
    let mut e = Env::new();
    let admin = e.admin.insecure_clone();
    let payer = e.payer.insecure_clone();
    // non-admin cannot create
    let mut ix = e.create_ruleset(1, KIND_PUMP, 0, a(PUMP));
    ix.accounts[0] = s(payer.pubkey());
    e.fails_with(&[ix], &[], "InvalidOwner");
    // bad params
    e.fails_with(&[e.create_ruleset(1, 9, 0, a(PUMP))], &[&admin], "InvalidRuleKind");
    e.fails_with(&[e.create_ruleset(1, KIND_PUMP, 0, Address::default())], &[&admin], "InvalidInput");
    e.fails_with(&[e.create_ruleset(1, KIND_PUMP, 4, a(PUMP))], &[&admin], "InvalidInput");
    e.fails_with(&[e.create_ruleset(1, KIND_ANY, 1, Address::default())], &[&admin], "InvalidInput");
    // admin creates all kinds
    e.ok(&[e.create_ruleset(1, KIND_PUMP, 0, a(PUMP))], &[&admin]);
    e.ok(&[e.create_ruleset(2, KIND_ANY, 0, Address::default())], &[&admin]);
    e.ok(&[e.create_ruleset(3, KIND_IMMUTABLE, 0, Address::default())], &[&admin]);
    let d = e.data(&e.ruleset(1));
    assert_eq!((d[11], d[12], pk(&d, 16)), (KIND_PUMP, 0, a(PUMP)));
    // duplicate index fails (init)
    let res = e.send(&[e.create_ruleset(1, KIND_PUMP, 0, a(PUMP))], &[&admin]);
    assert!(res.is_err());
    // update: admin ok, non-admin rejected, validation applies
    e.ok(&[e.update_ruleset(&admin.pubkey(), 1, KIND_PUMP, FLAG_ALLOW_MAYHEM | FLAG_REQUIRE_COMPLETE, a(PUMP))], &[&admin]);
    assert_eq!(e.data(&e.ruleset(1))[12], FLAG_ALLOW_MAYHEM | FLAG_REQUIRE_COMPLETE);
    e.fails_with(&[e.update_ruleset(&payer.pubkey(), 1, KIND_PUMP, 0, a(PUMP))], &[], "InvalidOwner");
    e.fails_with(&[e.update_ruleset(&admin.pubkey(), 1, 9, 0, a(PUMP))], &[&admin], "InvalidRuleKind");
}

// ---------------------------------------------------------------- collections

#[test]
fn collection_create_update_permissionless() {
    let mut e = Env::new();
    let admin = e.admin.insecure_clone();
    let payer = e.payer.insecure_clone();
    e.ok(&[e.create_ruleset(2, KIND_ANY, 0, Address::default())], &[&admin]);
    e.fails_with(&[e.create_collection(2, 7, a(WSOL), 0)], &[], "InvalidInput");
    e.ok(&[e.create_collection(2, 7, a(WSOL), 100)], &[]);
    let c = e.collection(&payer.pubkey(), 7);
    let d = e.data(&c);
    assert_eq!(u16::from_le_bytes(d[9..11].try_into().unwrap()), 7);
    assert_eq!(pk(&d, 16), payer.pubkey());
    assert_eq!(pk(&d, 48), e.ruleset(2));
    assert_eq!(pk(&d, 80), a(WSOL));
    assert_eq!(u32::from_le_bytes(d[112..116].try_into().unwrap()), 100);
    // ruleset must exist and be a Ruleset account
    let mut bad = e.create_collection(2, 8, a(WSOL), 100);
    bad.accounts[1] = r(c);
    e.fails_with(&[bad], &[], "AccountDiscriminatorMismatch");
    // update divisor; 0 rejected; invalid param rejected
    e.ok(&[e.update_collection(&payer.pubkey(), c, 0, 1000, None)], &[]);
    assert_eq!(u32::from_le_bytes(e.data(&c)[112..116].try_into().unwrap()), 1000);
    e.fails_with(&[e.update_collection(&payer.pubkey(), c, 0, 0, None)], &[], "InvalidInput");
    e.fails_with(&[e.update_collection(&payer.pubkey(), c, 5, 1, None)], &[], "InvalidInput");
    e.fails_with(&[e.update_collection(&payer.pubkey(), c, 1, 0, None)], &[], "InvalidInput");
    // transfer authority; old authority locked out
    let new_auth = Keypair::new();
    e.ok(&[e.update_collection(&payer.pubkey(), c, 1, 0, Some(new_auth.pubkey()))], &[]);
    assert_eq!(pk(&e.data(&c), 16), new_auth.pubkey());
    e.fails_with(&[e.update_collection(&payer.pubkey(), c, 0, 50, None)], &[], "InvalidOwner");
    e.ok(&[e.update_collection(&new_auth.pubkey(), c, 0, 50, None)], &[&new_auth]);
}

// ---------------------------------------------------------------- membership rules

#[test]
fn register_member_pump_fun_rule() {
    let mut e = Env::new();
    let admin = e.admin.insecure_clone();
    let payer = e.payer.insecure_clone();
    e.ok(&[e.create_ruleset(1, KIND_PUMP, 0, a(PUMP))], &[&admin]);
    e.ok(&[e.create_collection(1, 1, a(WSOL), 100)], &[]);
    let c = e.collection(&payer.pubkey(), 1);
    let rs = e.ruleset(1);
    let pump = e.pump_mint;
    let mayhem = e.mayhem_mint;
    // quote mint: no proof, no rule check
    e.ok(&[e.register(c, rs, a(WSOL), &[])], &[]);
    let d = e.data(&e.member(&c, &a(WSOL)));
    assert_eq!((pk(&d, 16), pk(&d, 48), u64::from_le_bytes(d[80..88].try_into().unwrap()), pk(&d, 88)), (c, a(WSOL), RATE_ONE, payer.pubkey()));
    // real standard launch with its real curve
    let cu = e.ok(&[e.register(c, rs, pump, &[e.curve(&pump)])], &[]).compute_units_consumed;
    assert!(cu < 30_000, "{cu}");
    assert_eq!(u32::from_le_bytes(e.data(&c)[116..120].try_into().unwrap()), 2, "member_count");
    // duplicate registration fails (init)
    assert!(e.send(&[e.register(c, rs, pump, &[e.curve(&pump)])], &[]).is_err());
    // real mayhem launch rejected
    e.fails_with(&[e.register(c, rs, mayhem, &[e.curve(&mayhem)])], &[], "RuleCheckFailed");
    // missing proof
    e.fails_with(&[e.register(c, rs, mayhem, &[])], &[], "RuleCheckFailed");
    // wrong curve for the mint (PDA mismatch)
    e.fails_with(&[e.register(c, rs, mayhem, &[e.curve(&pump)])], &[], "RuleCheckFailed");
    // ruleset account that is not the collection's ruleset
    e.ok(&[e.create_ruleset(2, KIND_ANY, 0, Address::default())], &[&admin]);
    e.fails_with(&[e.register(c, e.ruleset(2), mayhem, &[e.curve(&mayhem)])], &[], "InvalidInput");
    // fabricated curves at the right PDA
    let m1 = e.fake_mint(false, false);
    e.fake_curve(&m1, |d| d[81] = 0, a(SYSTEM)); // wrong owner
    e.fails_with(&[e.register(c, rs, m1, &[e.curve(&m1)])], &[], "RuleCheckFailed");
    e.fake_curve(&m1, |d| d[0] ^= 0xff, a(PUMP)); // wrong discriminator
    e.fails_with(&[e.register(c, rs, m1, &[e.curve(&m1)])], &[], "RuleCheckFailed");
    e.fake_curve(&m1, |d| d.truncate(81), a(PUMP)); // pre-mayhem `create` v1 curve: no flag byte, admitted
    e.ok(&[e.register(c, rs, m1, &[e.curve(&m1)])], &[]);
    let m2 = e.fake_mint(false, false);
    e.fake_curve(&m2, |d| d[81] = 1, a(PUMP)); // mayhem flag set
    e.fails_with(&[e.register(c, rs, m2, &[e.curve(&m2)])], &[], "RuleCheckFailed");
    // allow-mayhem ruleset admits it; require-complete ruleset rejects an incomplete curve
    e.ok(&[e.create_ruleset(3, KIND_PUMP, FLAG_ALLOW_MAYHEM, a(PUMP))], &[&admin]);
    e.ok(&[e.create_collection(3, 3, a(WSOL), 100)], &[]);
    let c3 = e.collection(&payer.pubkey(), 3);
    e.ok(&[e.register(c3, e.ruleset(3), m2, &[e.curve(&m2)])], &[]);
    e.ok(&[e.create_ruleset(4, KIND_PUMP, FLAG_REQUIRE_COMPLETE, a(PUMP))], &[&admin]);
    e.ok(&[e.create_collection(4, 4, a(WSOL), 100)], &[]);
    let c4 = e.collection(&payer.pubkey(), 4);
    let m3 = e.fake_mint(false, false);
    e.fake_curve(&m3, |d| d[48] = 0, a(PUMP)); // complete = false
    e.fails_with(&[e.register(c4, e.ruleset(4), m3, &[e.curve(&m3)])], &[], "RuleCheckFailed");
    e.fake_curve(&m3, |d| d[48] = 1, a(PUMP));
    e.ok(&[e.register(c4, e.ruleset(4), m3, &[e.curve(&m3)])], &[]);
}

#[test]
fn register_member_any_and_immutable_rules() {
    let mut e = Env::new();
    let admin = e.admin.insecure_clone();
    let payer = e.payer.insecure_clone();
    e.ok(&[e.create_ruleset(2, KIND_ANY, 0, Address::default()), e.create_ruleset(3, KIND_IMMUTABLE, 0, Address::default())], &[&admin]);
    e.ok(&[e.create_collection(2, 2, a(WSOL), 100), e.create_collection(3, 3, a(WSOL), 100)], &[]);
    let (c_any, c_imm) = (e.collection(&payer.pubkey(), 2), e.collection(&payer.pubkey(), 3));
    let with_auth = e.fake_mint(true, false);
    let with_freeze = e.fake_mint(false, true);
    let immutable = e.fake_mint(false, false);
    // Any admits everything, proof ignored
    e.ok(&[e.register(c_any, e.ruleset(2), with_auth, &[]), e.register(c_any, e.ruleset(2), immutable, &[Address::new_unique()])], &[]);
    // Immutable: authorities must both be revoked
    e.fails_with(&[e.register(c_imm, e.ruleset(3), with_auth, &[])], &[], "RuleCheckFailed");
    e.fails_with(&[e.register(c_imm, e.ruleset(3), with_freeze, &[])], &[], "RuleCheckFailed");
    e.ok(&[e.register(c_imm, e.ruleset(3), immutable, &[])], &[]);
    // the mint account must be a mint
    let mut bad = e.register(c_any, e.ruleset(2), e.user_wsol, &[]);
    bad.accounts[3] = r(e.user_wsol);
    assert!(e.send(&[bad], &[]).is_err());
}

#[test]
fn member_rate_authority() {
    let mut e = Env::new();
    let admin = e.admin.insecure_clone();
    let payer = e.payer.insecure_clone();
    e.ok(&[e.create_ruleset(2, KIND_ANY, 0, Address::default())], &[&admin]);
    e.ok(&[e.create_collection(2, 1, a(WSOL), 100), e.create_collection(2, 2, a(WSOL), 100)], &[]);
    let (c1, c2) = (e.collection(&payer.pubkey(), 1), e.collection(&payer.pubkey(), 2));
    let pump = e.pump_mint;
    e.ok(&[e.register(c1, e.ruleset(2), pump, &[]), e.register(c2, e.ruleset(2), pump, &[])], &[]);
    let (m1, m2) = (e.member(&c1, &pump), e.member(&c2, &pump));
    e.ok(&[e.set_rate(&payer.pubkey(), c1, m1, 1_150_000_000)], &[]);
    assert_eq!(u64::from_le_bytes(e.data(&m1)[80..88].try_into().unwrap()), 1_150_000_000);
    e.fails_with(&[e.set_rate(&payer.pubkey(), c1, m1, 0)], &[], "InvalidInput");
    e.fails_with(&[e.set_rate(&payer.pubkey(), c1, m2, 5)], &[], "InvalidCollectionMember");
    let other = Keypair::new();
    e.svm.airdrop(&other.pubkey(), 1_000_000_000).unwrap();
    e.fails_with(&[e.set_rate(&other.pubkey(), c1, m1, 5)], &[&other], "InvalidOwner");
}

// ---------------------------------------------------------------- rebalance swap on live pool state

#[test]
fn rebalance_swap_gating_and_fee() {
    let mut e = Env::new();
    let admin = e.admin.insecure_clone();
    let payer = e.payer.insecure_clone();
    e.ok(&[e.create_ruleset(1, KIND_PUMP, 0, a(PUMP))], &[&admin]);
    e.ok(&[e.create_collection(1, 1, a(WSOL), 100), e.create_collection(1, 2, a(WSOL), 500)], &[]);
    let (c, c500) = (e.collection(&payer.pubkey(), 1), e.collection(&payer.pubkey(), 2));
    let (rs, pump) = (e.ruleset(1), e.pump_mint);
    for coll in [c, c500] {
        e.ok(&[e.register(coll, rs, a(WSOL), &[]), e.register(coll, rs, pump, &[e.curve(&pump)])], &[]);
    }
    let rate = e.balanced_pump_rate();
    e.ok(&[e.set_rate(&payer.pubkey(), c, e.member(&c, &pump), rate), e.set_rate(&payer.pubkey(), c500, e.member(&c500, &pump), rate)], &[]);
    let sol = 1_000_000_000u64;

    // balanced pool (residual imbalance is below 0.1 SOL from integer rate rounding): trades of ~1 SOL of
    // value in either direction can only push it further off
    let one_sol_of_pump = (sol as u128 * 10u128.pow(e.pool.dec1 as u32) * RATE_ONE as u128 / (rate as u128 * 10u128.pow(9))) as u64;
    e.fails_with(&[e.rebalance(true, sol, c)], &[], "NotRebalancing");
    e.fails_with(&[e.rebalance(false, one_sol_of_pump, c)], &[], "NotRebalancing");

    // standard swap: unchanged behaviour, full fee, output matches x*y=k
    let (v0, v1) = e.reserves();
    let fee = ceil_fee(20 * sol, TRADE_FEE_RATE);
    let expected_out = (v1 as u128 * (20 * sol - fee) as u128 / (v0 as u128 + (20 * sol - fee) as u128)) as u64;
    let before = u64::from_le_bytes(e.data(&e.user_pump)[64..72].try_into().unwrap());
    let m = e.ok(&[e.swap(true, 20 * sol)], &[]);
    let got = u64::from_le_bytes(e.data(&e.user_pump)[64..72].try_into().unwrap()) - before;
    assert_eq!(got, expected_out);
    assert_eq!(swap_event_trade_fee(&m), fee);

    // same direction pushes further off balance
    e.fails_with(&[e.rebalance(true, sol, c)], &[], "NotRebalancing");
    // toward balance: accepted at fee / 100, then fee / 500 through the other collection
    let leg = got / 4;
    let m = e.ok(&[e.rebalance(false, leg, c)], &[]);
    assert_eq!(swap_event_trade_fee(&m), ceil_fee(leg, TRADE_FEE_RATE / 100));
    let m = e.ok(&[e.rebalance(false, leg, c500)], &[]);
    assert_eq!(swap_event_trade_fee(&m), ceil_fee(leg, TRADE_FEE_RATE / 500));
    // overshoot past balance is rejected
    e.fails_with(&[e.rebalance(false, got * 3, c)], &[], "NotRebalancing");
    // members must belong to the collection passed (PDA check)
    let mut bad = e.rebalance(false, leg, c);
    bad.accounts[14] = r(e.member(&c500, &pump));
    e.fails_with(&[bad], &[], "ConstraintSeeds");
    // a collection where only one mint is a member cannot be used
    e.ok(&[e.create_collection(1, 3, a(WSOL), 100)], &[]);
    let c3 = e.collection(&payer.pubkey(), 3);
    e.ok(&[e.register(c3, rs, a(WSOL), &[])], &[]);
    e.fails_with(&[e.rebalance(false, leg, c3)], &[], "AccountNotInitialized");
    // divisor larger than the fee rate -> fee floors at 1 ppm (never zero), still gated
    e.ok(&[e.update_collection(&payer.pubkey(), c, 0, 10_000, None)], &[]);
    let m = e.ok(&[e.rebalance(false, leg / 2, c)], &[]);
    assert_eq!(swap_event_trade_fee(&m), ceil_fee(leg / 2, 1));
}

// ---------------------------------------------------------------- LST rule: stake pool proof and rate sync

const STAKE_POOL_PROGRAM: &str = "SPoo1Ku8WFXoNDMHPsrGSTSG1Y47rzgn41SLUNakuHy";
/// SPL stake pool account: type 1, pool_mint @162, total_lamports @258, pool_token_supply @266
fn fake_stake_pool(e: &mut Env, mint: &Address, total_lamports: u64, supply: u64, owner: Address, account_type: u8) -> Address {
    let k = Address::new_unique();
    let mut d = vec![0u8; 611];
    d[0] = account_type;
    d[162..194].copy_from_slice(mint.as_ref());
    d[258..266].copy_from_slice(&total_lamports.to_le_bytes());
    d[266..274].copy_from_slice(&supply.to_le_bytes());
    e.svm.set_account(k, Account { lamports: 10_000_000, data: d, owner, executable: false, rent_epoch: 0 }).unwrap();
    k
}

#[test]
fn lst_rule_and_rate_sync() {
    let mut e = Env::new();
    let admin = e.admin.insecure_clone();
    let payer = e.payer.insecure_clone();
    let sp = a(STAKE_POOL_PROGRAM);
    e.fails_with(&[e.create_ruleset(5, 3, 1, sp)], &[&admin], "InvalidInput");
    e.fails_with(&[e.create_ruleset(5, 3, 0, Address::default())], &[&admin], "InvalidInput");
    e.ok(&[e.create_ruleset(5, 3, 0, sp)], &[&admin]);
    e.ok(&[e.create_collection(5, 5, a(WSOL), 100)], &[]);
    let (c, rs) = (e.collection(&payer.pubkey(), 5), e.ruleset(5));
    let lst = e.fake_mint(true, false); // LST mints keep their (stake pool) mint authority
    // 1.15 SOL per LST
    let pool = fake_stake_pool(&mut e, &lst, 1_150_000 * 1_000_000_000, 1_000_000 * 1_000_000_000, sp, 1);
    // proofs that must fail: missing, wrong owner, wrong type, wrong mint
    e.fails_with(&[e.register(c, rs, lst, &[])], &[], "RuleCheckFailed");
    let wrong_owner = fake_stake_pool(&mut e, &lst, 1, 1, a(SYSTEM), 1);
    e.fails_with(&[e.register(c, rs, lst, &[wrong_owner])], &[], "RuleCheckFailed");
    let wrong_type = fake_stake_pool(&mut e, &lst, 1, 1, sp, 2);
    e.fails_with(&[e.register(c, rs, lst, &[wrong_type])], &[], "RuleCheckFailed");
    let other_mint = e.fake_mint(true, false);
    let other_pool = fake_stake_pool(&mut e, &other_mint, 1, 1, sp, 1);
    e.fails_with(&[e.register(c, rs, lst, &[other_pool])], &[], "RuleCheckFailed");
    // valid: registers at the live exchange rate, not 1:1
    e.ok(&[e.register(c, rs, a(WSOL), &[]), e.register(c, rs, lst, &[pool])], &[]);
    let member = e.member(&c, &lst);
    let rate = |e: &Env| u64::from_le_bytes(e.data(&member)[80..88].try_into().unwrap());
    assert_eq!(rate(&e), 1_150_000_000);
    // the pool earns yield: anyone re-syncs the rate, no signer
    let mut pd = e.svm.get_account(&pool).unwrap();
    pd.data[258..266].copy_from_slice(&(1_200_000u64 * 1_000_000_000).to_le_bytes());
    e.svm.set_account(pool, pd).unwrap();
    let sync = |e: &Env, member: Address, proof: Address| e.ix(vec![r(c), r(rs), w(member), r(proof)], disc("global:sync_member_rate"));
    e.ok(&[sync(&e, member, pool)], &[]);
    assert_eq!(rate(&e), 1_200_000_000);
    // sync rejects: another pool, the quote member, a non-Lst ruleset
    e.fails_with(&[sync(&e, member, other_pool)], &[], "RuleCheckFailed");
    e.fails_with(&[sync(&e, e.member(&c, &a(WSOL)), pool)], &[], "InvalidCollectionMember");
    e.ok(&[e.create_ruleset(6, KIND_ANY, 0, Address::default())], &[&admin]);
    e.ok(&[e.create_collection(6, 6, a(WSOL), 100)], &[]);
    let c6 = e.collection(&payer.pubkey(), 6);
    e.ok(&[e.register(c6, e.ruleset(6), lst, &[])], &[]);
    e.fails_with(&[e.ix(vec![r(c6), r(e.ruleset(6)), w(e.member(&c6, &lst)), r(pool)], disc("global:sync_member_rate"))], &[], "InvalidRuleKind");
    // empty pool: rate falls back to 1:1
    let fresh = e.fake_mint(true, false);
    let empty = fake_stake_pool(&mut e, &fresh, 0, 0, sp, 1);
    e.ok(&[e.register(c, rs, fresh, &[empty])], &[]);
    assert_eq!(u64::from_le_bytes(e.data(&e.member(&c, &fresh))[80..88].try_into().unwrap()), RATE_ONE);
}

// ---------------------------------------------------------------- LaunchpadDbc rule: Meteora DBC virtual pool + config proof

const DBC_PROGRAM: &str = "dbcij3LWUppWqq96dh6gJWwBifmcGfLSB5D4DuSMaqN";
/// VirtualPool: disc | PoolState { volatility 64 | config @72 | creator @104 | base_mint @136 | ... } (424 bytes)
fn fake_virtual_pool(e: &mut Env, config: &Address, base_mint: &Address, owner: Address, bad_disc: bool) -> Address {
    let k = Address::new_unique();
    let mut d = vec![0u8; 424];
    d[..8].copy_from_slice(&[213, 224, 5, 209, 98, 69, 119, 92]);
    if bad_disc { d[0] ^= 0xff; }
    d[72..104].copy_from_slice(config.as_ref());
    d[136..168].copy_from_slice(base_mint.as_ref());
    e.svm.set_account(k, Account { lamports: 10_000_000, data: d, owner, executable: false, rent_epoch: 0 }).unwrap();
    k
}
/// PoolConfig: disc | quote_mint @8 | fee_claimer @40 | leftover_receiver @72 | ... (1048 bytes)
fn fake_pool_config(e: &mut Env, fee_claimer: &Address, owner: Address) -> Address {
    let k = Address::new_unique();
    let mut d = vec![0u8; 1048];
    d[..8].copy_from_slice(&[26, 108, 14, 123, 116, 230, 129, 43]);
    d[8..40].copy_from_slice(a(WSOL).as_ref());
    d[40..72].copy_from_slice(fee_claimer.as_ref());
    d[72..104].copy_from_slice(fee_claimer.as_ref());
    e.svm.set_account(k, Account { lamports: 10_000_000, data: d, owner, executable: false, rent_epoch: 0 }).unwrap();
    k
}

#[test]
fn launchpad_dbc_rule() {
    let mut e = Env::new();
    let admin = e.admin.insecure_clone();
    let payer = e.payer.insecure_clone();
    let partner = Address::new_unique(); // our launchpad partner PDA, the ruleset's program_id field
    let dbc = a(DBC_PROGRAM);
    e.fails_with(&[e.create_ruleset(7, 4, 1, partner)], &[&admin], "InvalidInput");
    e.fails_with(&[e.create_ruleset(7, 4, 0, Address::default())], &[&admin], "InvalidInput");
    e.ok(&[e.create_ruleset(7, 4, 0, partner)], &[&admin]);
    e.ok(&[e.create_collection(7, 7, a(WSOL), 100)], &[]);
    let (c, rs) = (e.collection(&payer.pubkey(), 7), e.ruleset(7));
    let meme = e.fake_mint(false, false);
    let our_cfg = fake_pool_config(&mut e, &partner, dbc);
    let vp = fake_virtual_pool(&mut e, &our_cfg, &meme, dbc, false);
    // negatives: missing proof, only one account, wrong owner on either, bad discriminator, config mismatch,
    // wrong mint, someone else's config (different fee claimer)
    e.fails_with(&[e.register(c, rs, meme, &[])], &[], "RuleCheckFailed");
    e.fails_with(&[e.register(c, rs, meme, &[vp])], &[], "RuleCheckFailed");
    let vp_sys = fake_virtual_pool(&mut e, &our_cfg, &meme, a(SYSTEM), false);
    e.fails_with(&[e.register(c, rs, meme, &[vp_sys, our_cfg])], &[], "RuleCheckFailed");
    let cfg_sys = fake_pool_config(&mut e, &partner, a(SYSTEM));
    let vp2 = fake_virtual_pool(&mut e, &cfg_sys, &meme, dbc, false);
    e.fails_with(&[e.register(c, rs, meme, &[vp2, cfg_sys])], &[], "RuleCheckFailed");
    let vp_bad = fake_virtual_pool(&mut e, &our_cfg, &meme, dbc, true);
    e.fails_with(&[e.register(c, rs, meme, &[vp_bad, our_cfg])], &[], "RuleCheckFailed");
    let other_cfg = fake_pool_config(&mut e, &partner, dbc);
    e.fails_with(&[e.register(c, rs, meme, &[vp, other_cfg])], &[], "RuleCheckFailed"); // pool points at our_cfg, not other_cfg
    let other_mint = e.fake_mint(false, false);
    e.fails_with(&[e.register(c, rs, other_mint, &[vp, our_cfg])], &[], "RuleCheckFailed");
    let foreign_cfg = fake_pool_config(&mut e, &Address::new_unique(), dbc);
    let vp_foreign = fake_virtual_pool(&mut e, &foreign_cfg, &meme, dbc, false);
    e.fails_with(&[e.register(c, rs, meme, &[vp_foreign, foreign_cfg])], &[], "RuleCheckFailed");
    // the real thing
    e.ok(&[e.register(c, rs, a(WSOL), &[]), e.register(c, rs, meme, &[vp, our_cfg])], &[]);
    let d = e.data(&e.member(&c, &meme));
    assert_eq!((pk(&d, 16), pk(&d, 48), u64::from_le_bytes(d[80..88].try_into().unwrap())), (c, meme, RATE_ONE));
}

#[test]
fn anchor_mint_is_admitted_without_rule() {
    let mut e = Env::new();
    let admin = e.admin.insecure_clone();
    let payer = e.payer.insecure_clone();
    let partner = Address::new_unique();
    e.ok(&[e.create_ruleset(8, 4, 0, partner)], &[&admin]);
    let protocol = e.fake_mint(true, false); // not a DBC launch, would fail the rule
    // collection with anchor_mint = protocol (remaining account 0)
    let mut ix = e.create_collection(8, 8, a(WSOL), 100);
    ix.accounts.push(r(protocol));
    e.ok(&[ix], &[]);
    let (c, rs) = (e.collection(&payer.pubkey(), 8), e.ruleset(8));
    assert_eq!(pk(&e.data(&c), 120), protocol, "anchor_mint stored");
    e.ok(&[e.register(c, rs, a(WSOL), &[]), e.register(c, rs, protocol, &[])], &[]);
    let stranger = e.fake_mint(true, false);
    e.fails_with(&[e.register(c, rs, stranger, &[])], &[], "RuleCheckFailed");
    // without an anchor the same mint is refused
    e.ok(&[e.create_collection(8, 9, a(WSOL), 100)], &[]);
    e.fails_with(&[e.register(e.collection(&payer.pubkey(), 9), rs, protocol, &[])], &[], "RuleCheckFailed");
}
