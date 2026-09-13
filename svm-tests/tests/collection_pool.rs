//! Collection pools: bind the live WSOL/pump pair to a pump collection, hang two more members off it,
//! and swap members directly inside the pool (intra swaps) at the collection rate.
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
const RENT: &str = "SysvarRent111111111111111111111111111111111";
const WSOL: &str = "So11111111111111111111111111111111111111112";
const TRADE_FEE_RATE: u64 = 2500;

fn a(s: &str) -> Address { Address::from_str(s).unwrap() }
fn disc(name: &str) -> Vec<u8> { Sha256::digest(name.as_bytes())[..8].to_vec() }
fn w(k: Address) -> AccountMeta { AccountMeta::new(k, false) }
fn r(k: Address) -> AccountMeta { AccountMeta::new_readonly(k, false) }
fn s(k: Address) -> AccountMeta { AccountMeta::new(k, true) }
fn pda(seeds: &[&[u8]], program: &Address) -> Address { Address::find_program_address(seeds, program).0 }
fn root() -> PathBuf { PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf() }
fn pk(d: &[u8], o: usize) -> Address { Address::new_from_array(d[o..o + 32].try_into().unwrap()) }

struct Env {
    svm: LiteSVM,
    program: Address,
    payer: Keypair,
    admin: Keypair,
    pool: Address,
    amm_config: Address,
    base_mint: Address,   // pump (token_1)
    base_vault: Address,
    tp_base: Address,
    quote_mint: Address,  // WSOL (token_0)
    user_base: Address,
    pump_curve: Address,
}
impl Env {
    fn new() -> Self {
        let program = a(PROGRAM);
        let so = std::fs::read(root().join("target/deploy/raydium_cp_swap.so")).unwrap();
        let admin_bytes: Vec<u8> = serde_json::from_str(&std::fs::read_to_string(root().join("svm-tests/fixtures/test-admin.json")).unwrap()).unwrap();
        let admin = Keypair::try_from(&admin_bytes[..]).unwrap();
        let mut svm = LiteSVM::new();
        svm.add_program(program, &so).unwrap();
        let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(root().join("svm-tests/fixtures/cpmm_wsol_pump.json")).unwrap()).unwrap();
        let mut accounts = HashMap::new();
        for (k, acc) in v["accounts"].as_object().unwrap() {
            let account = Account { lamports: acc["lamports"].as_u64().unwrap(), data: base64::engine::general_purpose::STANDARD.decode(acc["data"].as_str().unwrap()).unwrap(), owner: a(acc["owner"].as_str().unwrap()), executable: false, rent_epoch: 0 };
            svm.set_account(a(k), account.clone()).unwrap();
            accounts.insert(a(k), account);
        }
        let pool = a(v["pool"].as_str().unwrap());
        let d = &accounts[&pool].data;
        let (amm_config, vault1, mint0, mint1, tp1) = (pk(d, 8), pk(d, 104), pk(d, 168), pk(d, 200), pk(d, 264));
        let open_time = u64::from_le_bytes(d[373..381].try_into().unwrap());
        let mut clock: Clock = svm.get_sysvar();
        clock.unix_timestamp = open_time as i64 + 86_400;
        svm.set_sysvar(&clock);
        let payer = Keypair::new();
        svm.airdrop(&payer.pubkey(), 1_000 * 1_000_000_000).unwrap();
        svm.airdrop(&admin.pubkey(), 10 * 1_000_000_000).unwrap();
        // pool creator = payer so it can bind the pool
        let mut pd = accounts[&pool].clone();
        pd.data[40..72].copy_from_slice(payer.pubkey().as_ref());
        svm.set_account(pool, pd).unwrap();
        // user pump account cloned from the vault layout
        let user_base = Address::new_unique();
        let mut ud = accounts[&vault1].data.clone();
        ud[32..64].copy_from_slice(payer.pubkey().as_ref());
        ud[64..72].copy_from_slice(&(5_000_000_000u64 * 1_000_000).to_le_bytes());
        svm.set_account(user_base, Account { lamports: accounts[&vault1].lamports, data: ud, owner: tp1, executable: false, rent_epoch: 0 }).unwrap();
        let pump_curve = pda(&[b"bonding-curve", mint1.as_ref()], &a(PUMP));
        Env { svm, program, payer, admin, pool, amm_config, base_mint: mint1, base_vault: vault1, tp_base: tp1, quote_mint: mint0, user_base, pump_curve }
    }
    fn ix(&self, keys: Vec<AccountMeta>, data: Vec<u8>) -> Instruction { Instruction { program_id: self.program, accounts: keys, data } }
    fn send(&mut self, ixs: &[Instruction], signers: &[&Keypair]) -> Result<TransactionMetadata, FailedTransactionMetadata> {
        let budget = Instruction { program_id: a("ComputeBudget111111111111111111111111111111"), accounts: vec![], data: { let mut d = vec![2u8]; d.extend(1_400_000u32.to_le_bytes()); d } };
        let mut all_ixs = vec![budget]; all_ixs.extend_from_slice(ixs);
        let msg = Message::new_with_blockhash(&all_ixs, Some(&self.payer.pubkey()), &self.svm.latest_blockhash());
        let mut all: Vec<&Keypair> = vec![&self.payer];
        all.extend(signers.iter().filter(|k| k.pubkey() != self.payer.pubkey()));
        let tx = Transaction::new(&all, msg, self.svm.latest_blockhash());
        let res = self.svm.send_transaction(tx);
        self.svm.expire_blockhash();
        res
    }
    #[track_caller]
    fn ok(&mut self, ixs: &[Instruction], signers: &[&Keypair]) -> TransactionMetadata {
        match self.send(ixs, signers) { Ok(m) => m, Err(e) => panic!("expected success: {:?}\n{}", e.err, e.meta.logs.join("\n")) }
    }
    #[track_caller]
    fn fails_with(&mut self, ixs: &[Instruction], signers: &[&Keypair], code: &str) {
        match self.send(ixs, signers) {
            Ok(m) => panic!("expected {code}, but succeeded:\n{}", m.logs.join("\n")),
            Err(e) => assert!(e.meta.logs.iter().any(|l| l.contains(&format!("Error Code: {code}"))), "expected {code}, got {:?}\n{}", e.err, e.meta.logs.join("\n")),
        }
    }
    fn data(&self, k: &Address) -> Vec<u8> { self.svm.get_account(k).unwrap().data }
    fn amount(&self, k: &Address) -> u64 { u64::from_le_bytes(self.data(k)[64..72].try_into().unwrap()) }
    fn ruleset(&self, index: u16) -> Address { pda(&[b"ruleset", &index.to_le_bytes()], &self.program) }
    fn collection(&self, index: u16) -> Address { pda(&[b"token_collection", self.payer.pubkey().as_ref(), &index.to_le_bytes()], &self.program) }
    fn member(&self, c: &Address, m: &Address) -> Address { pda(&[b"collection_member", c.as_ref(), m.as_ref()], &self.program) }
    fn pool_members(&self) -> Address { pda(&[b"pool_members", self.pool.as_ref()], &self.program) }
    fn member_vault(&self, m: &Address) -> Address { pda(&[b"member_vault", self.pool.as_ref(), m.as_ref()], &self.program) }
    fn authority(&self) -> Address { pda(&[b"vault_and_lp_mint_auth_seed"], &self.program) }
    /// SPL mint with 6 decimals and no authorities, plus a funded user account for it.
    fn fake_member_mint(&mut self, user_amount: u64) -> (Address, Address) {
        let mint = Address::new_unique();
        let mut d = vec![0u8; 82]; d[44] = 6; d[45] = 1;
        self.svm.set_account(mint, Account { lamports: 1_461_600, data: d, owner: a(TOKEN), executable: false, rent_epoch: 0 }).unwrap();
        let acct = Address::new_unique();
        let mut t = vec![0u8; 165];
        t[0..32].copy_from_slice(mint.as_ref()); t[32..64].copy_from_slice(self.payer.pubkey().as_ref()); t[64..72].copy_from_slice(&user_amount.to_le_bytes()); t[108] = 1;
        self.svm.set_account(acct, Account { lamports: 2_039_280, data: t, owner: a(TOKEN), executable: false, rent_epoch: 0 }).unwrap();
        (mint, acct)
    }
    fn create_ruleset(&self, index: u16, kind: u8) -> Instruction {
        let mut d = disc("global:create_ruleset"); d.extend(index.to_le_bytes()); d.push(kind); d.push(0); d.extend(Address::default().as_ref());
        self.ix(vec![s(self.admin.pubkey()), w(self.ruleset(index)), r(a(SYSTEM))], d)
    }
    fn create_collection(&self, ruleset: u16, index: u16, divisor: u32) -> Instruction {
        let mut d = disc("global:create_token_collection"); d.extend(index.to_le_bytes()); d.extend(divisor.to_le_bytes());
        self.ix(vec![s(self.payer.pubkey()), r(self.ruleset(ruleset)), r(self.quote_mint), w(self.collection(index)), r(a(SYSTEM))], d)
    }
    fn register(&self, c: Address, rs: Address, mint: Address, proof: &[Address]) -> Instruction {
        let mut keys = vec![s(self.payer.pubkey()), w(c), r(rs), r(mint), w(self.member(&c, &mint)), r(a(SYSTEM))];
        keys.extend(proof.iter().map(|p| r(*p)));
        self.ix(keys, disc("global:register_collection_member"))
    }
    fn init_pool_members(&self, signer: &Address, c: Address, amp: u64) -> Instruction {
        let mut d = disc("global:init_pool_members"); d.extend(amp.to_le_bytes());
        self.ix(vec![s(*signer), r(self.pool), r(c), r(self.member(&c, &self.base_mint)), w(self.pool_members()), r(a(SYSTEM))], d)
    }
    fn add_member(&self, c: Address, mint: Address) -> Instruction {
        self.ix(vec![s(self.payer.pubkey()), r(self.authority()), r(self.pool), w(self.pool_members()), r(self.member(&c, &mint)), r(mint), w(self.member_vault(&mint)), r(a(TOKEN)), r(a(SYSTEM)), r(a(RENT))], disc("global:add_pool_member"))
    }
    /// members: (mint, user account, vault, token program) by index
    fn intra(&self, c: Address, members: &[(Address, Address, Address, Address)], i: usize, j: usize, amount: u64) -> Instruction {
        let mut keys = vec![s(self.payer.pubkey()), r(self.authority()), r(self.amm_config), w(self.pool), w(self.pool_members()), r(c), w(members[i].1), w(members[j].1), w(members[i].2), w(members[j].2), r(members[i].3), r(members[j].3), r(members[i].0), r(members[j].0)];
        for m in members { keys.push(r(m.2)); keys.push(r(self.member(&c, &m.0))); }
        let mut d = disc("global:intra_swap"); d.push(i as u8); d.push(j as u8); d.extend(amount.to_le_bytes()); d.extend(0u64.to_le_bytes());
        self.ix(keys, d)
    }
}
fn event_u64(m: &TransactionMetadata, name: &str, offset: usize) -> u64 {
    let d = disc(name);
    for l in &m.logs {
        if let Some(b64) = l.strip_prefix("Program data: ") {
            let bytes = base64::engine::general_purpose::STANDARD.decode(b64).unwrap();
            if bytes.starts_with(&d) { return u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap()); }
        }
    }
    panic!("no {name}");
}
// IntraSwapEvent: disc | pool 32 | in_mint 32 | out_mint 32 | input u64 @104 | output u64 @112 | trade_fee u64 @120
fn ceil_fee(amount: u64, rate: u64) -> u64 { ((amount as u128 * rate as u128 + 999_999) / 1_000_000) as u64 }

#[test]
fn collection_pool_intra_swaps() {
    let mut e = Env::new();
    let admin = e.admin.insecure_clone();
    let payer = e.payer.insecure_clone();
    // pump ruleset for the base, Any for the fabricated members (mirrors "a meme is a meme")
    e.ok(&[e.create_ruleset(1, 0)], &[&admin]);
    e.ok(&[e.create_collection(1, 1, 100)], &[]);
    let c = e.collection(1);
    let rs = e.ruleset(1);
    let base = e.base_mint;
    let (m1, u1) = e.fake_member_mint(10_000_000_000 * 1_000_000);
    let (m2, u2) = e.fake_member_mint(10_000_000_000 * 1_000_000);
    e.ok(&[e.register(c, rs, e.quote_mint, &[]), e.register(c, rs, base, &[e.pump_curve]), e.register(c, rs, m1, &[]), e.register(c, rs, m2, &[])], &[]);

    // bind the pair: only pool creator or admin; amp validated
    let other = Keypair::new();
    e.svm.airdrop(&other.pubkey(), 1_000_000_000).unwrap();
    e.fails_with(&[e.init_pool_members(&other.pubkey(), c, 200)], &[&other], "InvalidOwner");
    e.fails_with(&[e.init_pool_members(&payer.pubkey(), c, 0)], &[], "InvalidAmp");
    e.ok(&[e.init_pool_members(&payer.pubkey(), c, 200)], &[]);
    let pm = e.data(&e.pool_members());
    assert_eq!((pm[9], pm[10]), (0, 1), "base is token_1, n = 1");
    assert_eq!(pk(&pm, 16), e.pool);
    assert_eq!(pk(&pm, 48), c);
    assert_eq!(pk(&pm, 88), base, "member 0 mint");
    assert_eq!(pk(&pm, 120), e.base_vault, "member 0 vault = pool vault");

    // add members: not a collection member -> fails; duplicate -> fails; decimals mismatch -> fails
    let (stranger, _) = e.fake_member_mint(1);
    assert!(e.send(&[e.add_member(c, stranger)], &[]).is_err());
    e.ok(&[e.add_member(c, m1), e.add_member(c, m2)], &[]);
    e.fails_with(&[e.add_member(c, m1)], &[], "PoolMemberExists");
    let nine = Address::new_unique();
    let mut d = vec![0u8; 82]; d[44] = 9; d[45] = 1;
    e.svm.set_account(nine, Account { lamports: 1_461_600, data: d, owner: a(TOKEN), executable: false, rent_epoch: 0 }).unwrap();
    e.ok(&[e.register(c, rs, nine, &[])], &[]);
    e.fails_with(&[e.add_member(c, nine)], &[], "MemberDecimalsMismatch");
    assert_eq!(e.data(&e.pool_members())[10], 3, "n = 3");

    let members = [(base, e.user_base, e.base_vault, e.tp_base), (m1, u1, e.member_vault(&m1), a(TOKEN)), (m2, u2, e.member_vault(&m2), a(TOKEN))];
    let unit = 1_000_000u64;
    // member vaults are empty: the invariant is undefined with a zero balance, both directions refuse
    e.fails_with(&[e.intra(c, &members, 0, 1, 1_000 * unit)], &[], "StableCurveConvergence");
    e.fails_with(&[e.intra(c, &members, 1, 0, 1_000 * unit)], &[], "StableCurveConvergence");
    // seed the member vaults directly (as a first LP deposit would) so the curve has all balances
    for (mint, _, vault, _) in &members[1..] {
        let mut t = e.data(vault);
        t[64..72].copy_from_slice(&(50_000_000u64 * unit).to_le_bytes());
        let acc = e.svm.get_account(vault).unwrap();
        e.svm.set_account(*vault, Account { data: t, ..acc }).unwrap();
        let _ = mint;
    }
    // m1 -> base near the 1:1 rate (the pool holds ~93M base vs 50M m1, so m1 is slightly dear), fee/100
    let before = e.amount(&e.user_base);
    let m = e.ok(&[e.intra(c, &members, 1, 0, 1_000 * unit)], &[]);
    let got = e.amount(&e.user_base) - before;
    assert!(got > 980 * unit && got < 1_020 * unit, "{got}");
    assert_eq!(event_u64(&m, "event:IntraSwapEvent", 120), ceil_fee(1_000 * unit, TRADE_FEE_RATE / 100));
    // member fee accounting on PoolMembers, base fees on PoolState
    let pm = e.data(&e.pool_members());
    let m1_protocol = u64::from_le_bytes(pm[88 + 104 + 72..88 + 104 + 80].try_into().unwrap());
    assert_eq!(m1_protocol, ceil_fee(1_000 * unit, TRADE_FEE_RATE / 100) * 12 / 100);
    // base -> m2 and m1 -> m2 both work; m2 -> m1 too
    let b2 = e.amount(&u2);
    e.ok(&[e.intra(c, &members, 0, 2, 1_000 * unit), e.intra(c, &members, 1, 2, 500 * unit), e.intra(c, &members, 2, 1, 200 * unit)], &[]);
    let net = e.amount(&u2) - b2;
    assert!(net > 1_280 * unit && net < 1_320 * unit, "net m2 received {net}");
    // the curve bends: two identical large sales of m1, the second (into an m1-heavier pool) pays less base
    let before = e.amount(&e.user_base);
    e.ok(&[e.intra(c, &members, 1, 0, 20_000_000 * unit)], &[]);
    let first = e.amount(&e.user_base) - before;
    let before = e.amount(&e.user_base);
    e.ok(&[e.intra(c, &members, 1, 0, 20_000_000 * unit)], &[]);
    let second = e.amount(&e.user_base) - before;
    assert!(second < first && first - second > first / 1000, "price must move against the seller as the pool skews: {first} then {second}");
    // buying out a member vault is possible only at a ruinous premium: 100x the reserve in base buys
    // (almost) the reserve in m2, i.e. the pool is paid ~100x the rate and never goes to zero
    let m2_reserve = e.amount(&members[2].2);
    let before = e.amount(&u2);
    let paid = 5_000_000_000 * unit;
    e.ok(&[e.intra(c, &members, 0, 2, paid)], &[]);
    let got = e.amount(&u2) - before;
    assert!(got < m2_reserve && e.amount(&members[2].2) > 0 && paid / got >= 50, "extracted {got} of {m2_reserve} for {paid}");
    // wrong vault / wrong index
    // wrong vault for the index: the mint constraint catches it before the member check
    let mut bad = e.intra(c, &members, 1, 0, unit);
    bad.accounts[8] = w(members[2].2);
    e.fails_with(&[bad], &[], "ConstraintAddress");
    // out-of-range index
    let mut bad = e.intra(c, &members, 1, 0, unit);
    bad.data[8] = 5;
    e.fails_with(&[bad], &[], "InvalidPoolMember");
    // collect member protocol fees: admin ok, stranger rejected, kind 2 rejected
    let recipient = Address::new_unique();
    let mut t = vec![0u8; 165]; t[0..32].copy_from_slice(m1.as_ref()); t[32..64].copy_from_slice(admin.pubkey().as_ref()); t[108] = 1;
    e.svm.set_account(recipient, Account { lamports: 2_039_280, data: t, owner: a(TOKEN), executable: false, rent_epoch: 0 }).unwrap();
    let collect = |e: &Env, signer: Address, kind: u8| { let mut d = disc("global:collect_member_fees"); d.push(1); d.push(kind); e.ix(vec![s(signer), r(e.authority()), r(e.amm_config), r(e.pool), w(e.pool_members()), w(e.member_vault(&m1)), w(recipient), r(m1), r(a(TOKEN))], d) };
    e.fails_with(&[collect(&e, payer.pubkey(), 0)], &[], "InvalidOwner");
    e.fails_with(&[collect(&e, admin.pubkey(), 2)], &[&admin], "InvalidInput");
    e.ok(&[collect(&e, admin.pubkey(), 0)], &[&admin]);
    assert!(e.amount(&recipient) > 0);
    e.fails_with(&[collect(&e, admin.pubkey(), 0)], &[&admin], "NoFeeCollect");
}
