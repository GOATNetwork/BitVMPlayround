use bitcoin::{Network, PublicKey};
use bitvm::{bridge::graphs::base, groth16::g16};
use bitvm::bridge::contexts::{
    base::generate_keys_from_secret,
    depositor::DepositorContext,
    verifier::VerifierContext,
    operator::OperatorContext,
};

pub const PROOF_PATH: &str = "poc_data/dummy_proof.json";
pub const COMPILE_PATH: &str = "poc_data/compile";
pub const TAPSCRIPT_PATH: &str = "poc_data/tapscripts";
pub const WOTS_SIGNATURE_PATH: &str = "poc_data/signed_assertions";
pub const WOTS_SECRET: &str = "a138982ce17ac813d505a5b40b665d404e9528e7"; // just for test
pub const N_TAPLEAVES: usize = g16::N_TAPLEAVES;
pub const N_ASSERTIONS: usize = g16::N_VERIFIER_PUBLIC_INPUTS + g16::N_VERIFIER_FQS + g16::N_VERIFIER_HASHES;

pub const TX_WAIT_TIME: u64 = 1; // in seconds
pub const RPCUSER: &str = "test";
pub const RPCPASSWORD: &str = "TEST_ONLY_1vvudX2YIVU6PUNxLxQT0sEQd4OjOyHF";
pub const RPC_URL: &str = "http://3.15.141.150:18453/wallet/public_test";

pub const PEGIN_AMOUNT: u64 = 100_000_000;
pub const KICKOFF_AMOUNT: u64 = 20_000_000;
pub const CHALLENGE_AMOUNT: u64 = 10_000_000;
pub const OPERATOR_SECRET: &str = base::OPERATOR_SECRET;
pub const VERIFIER_0_SECRET: &str = base::VERIFIER_0_SECRET;
pub const VERIFIER_1_SECRET: &str = base::VERIFIER_1_SECRET;
pub const DEPOSITOR_SECRET: &str = base::DEPOSITOR_SECRET;
pub const WITHDRAWER_SECRET: &str = base::WITHDRAWER_SECRET;
pub const DEPOSITOR_EVM_ADDRESS: &str = base::DEPOSITOR_EVM_ADDRESS;
pub const WITHDRAWER_EVM_ADDRESS: &str = base::WITHDRAWER_EVM_ADDRESS;

pub fn network() -> Network {
    Network::Regtest
}

pub fn get_depositor_context() -> DepositorContext {
    let (_, _, verifier_0_public_key) =
        generate_keys_from_secret(network(), VERIFIER_0_SECRET);
    let (_, _, verifier_1_public_key) =
        generate_keys_from_secret(network(), VERIFIER_1_SECRET);
    let mut n_of_n_public_keys: Vec<PublicKey> = Vec::new();
    n_of_n_public_keys.push(verifier_0_public_key);
    n_of_n_public_keys.push(verifier_1_public_key);
    DepositorContext::new(network(), DEPOSITOR_SECRET, &n_of_n_public_keys)
}

pub fn get_verifier_contexts() -> [VerifierContext; 2] {
    let (_, _, verifier_0_public_key) =
        generate_keys_from_secret(network(), VERIFIER_0_SECRET);
    let (_, _, verifier_1_public_key) =
        generate_keys_from_secret(network(), VERIFIER_1_SECRET);
    let mut n_of_n_public_keys: Vec<PublicKey> = Vec::new();
    n_of_n_public_keys.push(verifier_0_public_key);
    n_of_n_public_keys.push(verifier_1_public_key);

    let verifier_0_context =
        VerifierContext::new(network(), VERIFIER_0_SECRET, &n_of_n_public_keys);
    let verifier_1_context =
        VerifierContext::new(network(), VERIFIER_1_SECRET, &n_of_n_public_keys);

    [verifier_0_context, verifier_1_context]
}

pub fn get_operator_context() -> OperatorContext {
    let (_, _, verifier_0_public_key) =
        generate_keys_from_secret(network(), VERIFIER_0_SECRET);
    let (_, _, verifier_1_public_key) =
        generate_keys_from_secret(network(), VERIFIER_1_SECRET);
    let mut n_of_n_public_keys: Vec<PublicKey> = Vec::new();
    n_of_n_public_keys.push(verifier_0_public_key);
    n_of_n_public_keys.push(verifier_1_public_key);
    OperatorContext::new(network(), OPERATOR_SECRET, &n_of_n_public_keys)
}


