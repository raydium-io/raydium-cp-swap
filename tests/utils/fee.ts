import {
  MAX_FEE_BASIS_POINTS,
  ONE_IN_BASIS_POINTS,
  TOKEN_PROGRAM_ID,
} from "@solana/spl-token";
import { PublicKey } from "@solana/web3.js";

export function calculateFee(
  transferFeeConfig: { transferFeeBasisPoints: number; MaxFee: number },
  preFeeAmount: bigint,
  tokenProgram: PublicKey
): bigint {
  if (tokenProgram.equals(TOKEN_PROGRAM_ID)) {
    return BigInt(0);
  }
  if (preFeeAmount === BigInt(0)) {
    return BigInt(0);
  } else {
    const numerator =
      preFeeAmount * BigInt(transferFeeConfig.transferFeeBasisPoints);
    const rawFee =
      (numerator + ONE_IN_BASIS_POINTS - BigInt(1)) / ONE_IN_BASIS_POINTS;
    const fee =
      rawFee > transferFeeConfig.MaxFee ? transferFeeConfig.MaxFee : rawFee;
    return BigInt(fee);
  }
}

export function calculatePreFeeAmount(
  transferFeeConfig: { transferFeeBasisPoints: number; MaxFee: number },
  postFeeAmount: bigint,
  tokenProgram: PublicKey
) {
  if (
    transferFeeConfig.transferFeeBasisPoints == 0 ||
    tokenProgram.equals(TOKEN_PROGRAM_ID)
  ) {
    return postFeeAmount;
  } else {
    let numerator = postFeeAmount * BigInt(MAX_FEE_BASIS_POINTS);
    let denominator =
      MAX_FEE_BASIS_POINTS - transferFeeConfig.transferFeeBasisPoints;

    return (numerator + BigInt(denominator) - BigInt(1)) / BigInt(denominator);
  }
}
