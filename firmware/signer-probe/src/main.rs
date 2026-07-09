//! 在目标设备上跑，验证纯 Rust 密钥核心可运行且算对地址（对照官方向量）。
//! 期望输出（BIP-84 官方向量）：bip84[0]=bc1qcr8te4kr609gcawutmrza0j4xv80jy8z306fyu

use esp_signer_core::{btc_address, eth_address, mnemonic_to_seed, Net};

const ABANDON: &str =
    "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

fn main() {
    let seed = match mnemonic_to_seed(ABANDON, "") {
        Ok(s) => s,
        Err(e) => {
            println!("FAIL seed: {e}");
            return;
        }
    };
    match btc_address(&seed, Net::Mainnet, 0, 0, 0) {
        Ok(a) => println!("bip84[0]={a}"),
        Err(e) => println!("FAIL btc: {e}"),
    }
    match eth_address(&seed, 0, 0) {
        Ok(a) => println!("eth[0]={a}"),
        Err(e) => println!("FAIL eth: {e}"),
    }
    let ok = btc_address(&seed, Net::Mainnet, 0, 0, 0).map(|a| a
        == "bc1qcr8te4kr609gcawutmrza0j4xv80jy8z306fyu")
        .unwrap_or(false);
    println!("VECTOR_MATCH={ok}");
}
