use bitcoin::{Address, Amount, OutPoint, Txid, Weight, XOnlyPublicKey};
use bitcoincore_rpc::Client;
use bitvm::treepp::*;
use bitvm::bridge::{
    connectors::{revealer::Revealer, connector_c::ConnectorC}, 
    graphs::base::DUST_AMOUNT,
    scripts::{generate_pay_to_pubkey_script_address, generate_pay_to_pubkey_script}, 
    groth16::{
        load_proof_from_file,
        generate_wots_keys_from_secrets, assert_bitcom_lock,
        load_all_assert_tapscripts_from_file,
        load_all_signed_assertions_from_file,
        corrupt_signed_assertions, validate_assertions,
        assert_unlock_scripts_from_file,
        extract_signed_assertions_from_assert_tx,
        WotsSignatures, WotsPublicKeys, WotsSecretKeys, VerifyingKey,
    },
    transactions::{
        base::{BaseTransaction, Input, InputWithScript}, 
        kick_off_1::KickOff1Transaction, 
        kick_off_2::KickOff2Transaction, 
        peg_in_confirm::PegInConfirmTransaction, 
        peg_in_deposit::PegInDepositTransaction,
        challenge::ChallengeTransaction,
        take_1::Take1Transaction,
        take_2::Take2Transaction,
        assert::AssertTransaction,
        disprove::DisproveTransaction,
    }
};
use crate::{config::{self, network}, utils};



// return: peg_in_txid, peg_in_tx_weight
pub fn peg_in(rpc: &Client) -> (Txid, Weight) {
    let deposit_input_amount = Amount::from_sat(config::PEGIN_AMOUNT);

    let depositor_context = config::get_depositor_context();
    let verifier_contexts = config::get_verifier_contexts();
    let deposit_funding_utxo_address = generate_pay_to_pubkey_script_address(
        depositor_context.network,
        &depositor_context.depositor_public_key,
    );
    let deposit_funding_outpoint =
        utils::generate_stub_outpoint(rpc, &deposit_funding_utxo_address, deposit_input_amount);
    let deposit_input = Input {
        outpoint: deposit_funding_outpoint,
        amount: deposit_input_amount,
    };

    // peg-in deposit
    let peg_in_deposit =
        PegInDepositTransaction::new(&depositor_context, config::DEPOSITOR_EVM_ADDRESS, deposit_input);
    let peg_in_deposit_tx = peg_in_deposit.finalize();
    let deposit_txid = peg_in_deposit_tx.compute_txid();
    utils::broadcast_tx(rpc, &peg_in_deposit_tx);
    utils::mint_block(rpc, 1);
    utils::validate_tx(rpc, deposit_txid);

    // peg-in confirm
    let output_index = 0;
    let confirm_funding_outpoint = OutPoint {
        txid: deposit_txid,
        vout: output_index,
    };
    let confirm_input = Input {
        outpoint: confirm_funding_outpoint,
        amount: peg_in_deposit_tx.output[output_index as usize].value,
    };
    let mut peg_in_confirm =
        PegInConfirmTransaction::new(&depositor_context, config::DEPOSITOR_EVM_ADDRESS, confirm_input);

    let secret_nonces_0 = peg_in_confirm.push_nonces(&verifier_contexts[0]);
    let secret_nonces_1 = peg_in_confirm.push_nonces(&verifier_contexts[1]);

    peg_in_confirm.pre_sign(&verifier_contexts[0], &secret_nonces_0);
    peg_in_confirm.pre_sign(&verifier_contexts[1], &secret_nonces_1);

    let peg_in_confirm_tx = peg_in_confirm.finalize();
    let confirm_txid = peg_in_confirm_tx.compute_txid();
    let comfirm_tx_weight = peg_in_confirm_tx.weight();
    utils::broadcast_tx(rpc, &peg_in_confirm_tx);
    utils::mint_block(rpc, 1);
    utils::validate_tx(rpc, confirm_txid);
    (confirm_txid, comfirm_tx_weight)
}

// return: ((kickoff_1_txid, kickoff_1_tx_weigth), (kickoff_2_txid, kickoff_2_tx_weigth))
pub fn kick_off(rpc: &Client, bitcom_lock_scripts: &Vec<Script>) -> ((Txid, Weight), (Txid, Weight)) {
    let operator_context = config::get_operator_context();
    let kick_off_1_input_amount = Amount::from_sat(config::KICKOFF_AMOUNT);
    let funding_address = generate_pay_to_pubkey_script_address(
        operator_context.network,
        &operator_context.operator_public_key,
    );
    let funding_outpoint = utils::generate_stub_outpoint(rpc, &funding_address, kick_off_1_input_amount);
    let input = Input {
        outpoint: funding_outpoint,
        amount: kick_off_1_input_amount,
    };
    let kick_off_1_tx = KickOff1Transaction::new(&operator_context, input);
    let tx = kick_off_1_tx.finalize();
    let kick_off_1_txid = tx.compute_txid();
    let kick_off_1_tx_weight = tx.weight();
    utils::broadcast_tx(rpc, &tx);
    utils::mint_block(rpc, 1);
    utils::validate_tx(rpc, kick_off_1_txid);

    let connector_1_vout = 1;
    let connector_1_amount = utils::get_utxo_value(rpc, kick_off_1_txid, connector_1_vout);
    let revealers = get_revealers(&operator_context.n_of_n_taproot_public_key, bitcom_lock_scripts);
    let kick_off_2_tx = KickOff2Transaction::new(
        &operator_context,
        Input {
            outpoint: OutPoint{
                txid: kick_off_1_txid,
                vout: connector_1_vout,
            },
            amount: connector_1_amount,
        },
        revealers,
    );
    let tx = kick_off_2_tx.finalize();
    let kick_off_2_txid = tx.compute_txid();
    let kick_off_2_tx_weight = tx.weight();
    utils::broadcast_tx(rpc, &tx);
    utils::mint_block(rpc, 1);
    utils::validate_tx(rpc, kick_off_1_txid);

    (
        (kick_off_1_txid, kick_off_1_tx_weight),
        (kick_off_2_txid, kick_off_2_tx_weight),
    )
}

// return: (take_1_txid, take_1_tx_weight)
pub fn take_1(rpc: &Client, peg_in_txid: Txid, kick_off_1_txid: Txid, kick_off_2_txid: Txid) -> (Txid, Weight) {
    let operator_context = config::get_operator_context();
    let verifier_contexts = config::get_verifier_contexts();

    let connector_0_vout = 0; 
    let connector_0_amount = utils::get_utxo_value(rpc, peg_in_txid, connector_0_vout);
    let take_1_input_0 = Input {
        outpoint: OutPoint {
            txid: peg_in_txid,
            vout: connector_0_vout,
        },
        amount: connector_0_amount,
    };
    let connector_a_vout = 0; 
    let connector_a_amount = utils::get_utxo_value(rpc, kick_off_1_txid, connector_a_vout);
    let take_1_input_1 = Input {
        outpoint: OutPoint {
            txid: kick_off_1_txid,
            vout: connector_a_vout,
        },
        amount: connector_a_amount,
    };
    let connector_3_vout = 0; 
    let connector_3_amount = utils::get_utxo_value(rpc, kick_off_2_txid, connector_3_vout);
    let take_1_input_2 = Input {
        outpoint: OutPoint {
            txid: kick_off_2_txid,
            vout: connector_3_vout,
        },
        amount: connector_3_amount,
    };
    let connector_b_vout = 1; 
    let connector_b_amount = utils::get_utxo_value(rpc, kick_off_2_txid, connector_b_vout);
    let take_1_input_3 = Input {
        outpoint: OutPoint {
            txid: kick_off_2_txid,
            vout: connector_b_vout,
        },
        amount: connector_b_amount,
    };
    let mut take_1_tx = Take1Transaction::new(
        &operator_context,
        take_1_input_0,
        take_1_input_1,
        take_1_input_2,
        take_1_input_3,
    );

    let secret_nonces_0 = take_1_tx.push_nonces(&verifier_contexts[0]);
    let secret_nonces_1 = take_1_tx.push_nonces(&verifier_contexts[1]);

    take_1_tx.pre_sign(&verifier_contexts[0], &secret_nonces_0);
    take_1_tx.pre_sign(&verifier_contexts[1], &secret_nonces_1);

    let tx = take_1_tx.finalize();
    let take_1_txid = tx.compute_txid();
    let take_1_tx_weight = tx.weight();
    utils::broadcast_tx(&rpc, &tx);
    utils::mint_block(&rpc, 1);
    utils::validate_tx(&rpc, take_1_txid);
    (take_1_txid, take_1_tx_weight)
}

// return: (challenge_txid, challenge_tx_weight)
pub fn challenge(rpc: &Client, kick_off_1_txid: Txid) -> (Txid, Weight) {
    let depositor_context = config::get_depositor_context();
    let operator_context = config::get_operator_context();
    let connector_a_vout = 0;
    let connector_a_amount = utils::get_utxo_value(rpc, kick_off_1_txid, connector_a_vout);
    // re-use the depositor private key to imitate a third-party
    let crowdfunding_keypair = &depositor_context.depositor_keypair;
    let crowdfunding_public_key = &depositor_context.depositor_public_key;
    let challenge_amount = Amount::from_sat(config::CHALLENGE_AMOUNT);
    let challenger_address = generate_pay_to_pubkey_script_address(config::network(), crowdfunding_public_key);
    let funding_outpoint = utils::generate_stub_outpoint(rpc, &challenger_address, challenge_amount);
    let refund_address = generate_pay_to_pubkey_script_address(network(), crowdfunding_public_key);
    let mut challenge_tx = ChallengeTransaction::new(
        &operator_context,
        Input {
            outpoint: OutPoint{
                txid: kick_off_1_txid,
                vout: connector_a_vout
            },
            amount: connector_a_amount,
        },
        challenge_amount,
    );
    challenge_tx.add_inputs_and_output(
        &depositor_context,
        &vec![
            InputWithScript {
                outpoint: funding_outpoint,
                amount: challenge_amount,
                script: &generate_pay_to_pubkey_script(crowdfunding_public_key),
            },
        ],
        crowdfunding_keypair,
        refund_address.script_pubkey(),
    );
    let tx = challenge_tx.finalize();
    let challenge_txid = tx.compute_txid();
    let challenge_tx_weight = tx.weight();
    utils::broadcast_tx(&rpc, &tx);
    utils::mint_block(&rpc, 1);
    utils::validate_tx(&rpc, challenge_txid);
    (challenge_txid, challenge_tx_weight)
}

// return: ((assert_txid, assert_tx_weight), connector_c_address)
pub fn assert(
    rpc: &Client, 
    kick_off_2_txid: Txid, 
    bitcom_lock_scripts: &Vec<Script>,
    connector_c_tapscripts: &Vec<Script>,
    corrupt_index: Option<usize>,
) -> ((Txid, Weight), Address) {
    let operator_context = config::get_operator_context();
    let connector_b_vout = 1; 
    let connector_b_amount = utils::get_utxo_value(rpc, kick_off_2_txid, connector_b_vout);
    let assert_input_0 = Input {
        outpoint: OutPoint {
            txid: kick_off_2_txid,
            vout: connector_b_vout,
        },
        amount: connector_b_amount,
    };
    let mut connector_c = ConnectorC::new(network(), &operator_context.operator_taproot_public_key, &connector_c_tapscripts);
    let connector_c_address = connector_c.gen_taproot_address();
    
    let revealers = get_revealers(&operator_context.n_of_n_taproot_public_key, bitcom_lock_scripts);
    let bitcom_unlock_scripts = match corrupt_index {
        Some(index) => get_corrupt_bitcom_unlock_scripts(index),
        _ => get_bitcom_unlock_scripts(),
    };
    let bitcom_inputs = (0..bitcom_unlock_scripts.len())
        .map(|i| Input{
            outpoint: OutPoint {
                txid: kick_off_2_txid,
                vout: (i+2) as u32,
            },
            amount: Amount::from_sat(DUST_AMOUNT),
        })
        .collect();

    let mut assert_tx = AssertTransaction::new(
        &operator_context, 
        assert_input_0, 
        bitcom_inputs,
        connector_c, 
        revealers
    );
    assert_tx.push_bitcommitments_witness(bitcom_unlock_scripts);
    let tx = assert_tx.finalize();
    let assert_txid = tx.compute_txid();
    let assert_tx_weight = tx.weight();
    utils::broadcast_tx(&rpc, &tx);
    utils::mint_block(&rpc, 1);
    utils::validate_tx(&rpc, assert_txid);
    ((assert_txid, assert_tx_weight), connector_c_address)
}   

// return: (take_2_txid, take_2_tx_weight)
pub fn take_2(
    rpc: &Client, 
    peg_in_txid: Txid, 
    assert_txid: Txid, 
    connector_c_tapscripts: &Vec<Script>,
    connector_c_address: Option<Address>,
) -> (Txid, Weight) {
    let operator_context = config::get_operator_context();
    let verifier_contexts = config::get_verifier_contexts();

    let connector_0_vout = 0; 
    let connector_0_amount = utils::get_utxo_value(rpc, peg_in_txid, connector_0_vout);
    let take_2_input_0 = Input {
        outpoint: OutPoint {
            txid: peg_in_txid,
            vout: connector_0_vout,
        },
        amount: connector_0_amount,
    };

    let connector_4_vout  = 0;
    let connector_4_amount = utils::get_utxo_value(rpc, assert_txid, connector_4_vout);
    let take_2_input_1 = Input {
        outpoint: OutPoint {
            txid: assert_txid,
            vout: connector_4_vout,
        },
        amount: connector_4_amount,
    };

    let connector_5_vout  = 1;
    let connector_5_amount = utils::get_utxo_value(rpc, assert_txid, connector_5_vout);
    let take_2_input_2 = Input {
        outpoint: OutPoint {
            txid: assert_txid,
            vout: connector_5_vout,
        },
        amount: connector_5_amount,
    };

    let connector_c_vout  = 2;
    let connector_c_amount = utils::get_utxo_value(rpc, assert_txid, connector_c_vout);
    let take_2_input_3 = Input {
        outpoint: OutPoint {
            txid: assert_txid,
            vout: connector_c_vout,
        },
        amount: connector_c_amount,
    };

    let mut connector_c = ConnectorC::new(network(), &operator_context.operator_taproot_public_key, &connector_c_tapscripts);
    match connector_c_address {
        Some(addr) => connector_c.import_taproot_address(addr),
        _ => { connector_c.gen_taproot_address(); },
    };

    let mut take_2_tx = Take2Transaction::new(
        &operator_context,
        connector_c,
        take_2_input_0,
        take_2_input_1,
        take_2_input_2,
        take_2_input_3,
    );

    let secret_nonces_0 = take_2_tx.push_nonces(&verifier_contexts[0]);
    let secret_nonces_1 = take_2_tx.push_nonces(&verifier_contexts[1]);

    take_2_tx.pre_sign(&verifier_contexts[0], &secret_nonces_0);
    take_2_tx.pre_sign(&verifier_contexts[1], &secret_nonces_1);

    let tx = take_2_tx.finalize();
    let take_2_txid = tx.compute_txid();
    let take_2_tx_weight = tx.weight();
    utils::broadcast_tx(&rpc, &tx);
    utils::mint_block(&rpc, 1);
    utils::validate_tx(&rpc, take_2_txid);
    (take_2_txid, take_2_tx_weight)
}

// return (disprove_txid, disprove_tx_weight)
pub fn disprove(
    rpc: &Client, 
    assert_txid: Txid, 
    connector_c_tapscripts: &Vec<Script>,
    connector_c_address: Option<Address>,
) -> (Txid, Weight) {
    let operator_context = config::get_operator_context();
    let verifier_contexts = config::get_verifier_contexts();

    let connector_5_vout  = 1;
    let connector_5_amount = utils::get_utxo_value(rpc, assert_txid, connector_5_vout);
    let disprove_input_0 = Input {
        outpoint: OutPoint {
            txid: assert_txid,
            vout: connector_5_vout,
        },
        amount: connector_5_amount,
    };

    let connector_c_vout  = 2;
    let connector_c_amount = utils::get_utxo_value(rpc, assert_txid, connector_c_vout);
    let disprove_input_1 = Input {
        outpoint: OutPoint {
            txid: assert_txid,
            vout: connector_c_vout,
        },
        amount: connector_c_amount,
    };

    let mut connector_c = ConnectorC::new(network(), &operator_context.operator_taproot_public_key, &connector_c_tapscripts);
    match connector_c_address {
        Some(addr) => connector_c.import_taproot_address(addr),
        _ => { connector_c.gen_taproot_address(); },
    };

    let res = validate_assert_bitcom(rpc, assert_txid);
    let (leaf_index, hint_script) = match res {
        Some(v) => v,
        _ => panic!("bitcommitments in given assert_tx is completely valid, cannot disprove valid assertions"),
    };

    let mut disprove_tx = DisproveTransaction::new(
        &operator_context,
        connector_c,
        disprove_input_0,
        disprove_input_1,
        leaf_index as u32,
    );

    let secret_nonces_0 = disprove_tx.push_nonces(&verifier_contexts[0]);
    let secret_nonces_1 = disprove_tx.push_nonces(&verifier_contexts[1]);

    disprove_tx.pre_sign(&verifier_contexts[0], &secret_nonces_0);
    disprove_tx.pre_sign(&verifier_contexts[1], &secret_nonces_1);

    // re-use verifier_0 as challenger
    let challenger_reward_address = generate_pay_to_pubkey_script_address(
        verifier_contexts[0].network,
        &verifier_contexts[0].verifier_public_key,
    );
    let challenger_reward_script = challenger_reward_address.script_pubkey(); 
    disprove_tx.add_input_output(leaf_index as u32, challenger_reward_script, hint_script);

    let tx = disprove_tx.finalize();
    let disprove_txid = tx.compute_txid();
    let disprove_tx_weight = tx.weight();
    utils::broadcast_tx(&rpc, &tx);
    utils::mint_block(&rpc, 1);
    utils::validate_tx(&rpc, disprove_txid);
    (disprove_txid, disprove_tx_weight)
}




pub fn get_bitcom_lock_scripts() -> Vec<Script> {
    let (wots_pk, _) = get_wots_keys();
    assert_bitcom_lock(&wots_pk)
}

pub fn get_bitcom_unlock_scripts() -> Vec<Script> {
    assert_unlock_scripts_from_file(config::WOTS_SIGNATURE_PATH, None, None)
}

pub fn get_corrupt_bitcom_unlock_scripts(corrupt_index: usize) -> Vec<Script> {
    let (_, wots_sk) = get_wots_keys();
    assert_unlock_scripts_from_file(config::WOTS_SIGNATURE_PATH, Some(corrupt_index), Some(wots_sk))
}

pub fn get_assert_tapscripts() -> Vec<Script> {
    load_all_assert_tapscripts_from_file(config::TAPSCRIPT_PATH)
}

pub fn get_signed_assertions() -> WotsSignatures {
    load_all_signed_assertions_from_file(config::WOTS_SIGNATURE_PATH)
}   

pub fn corrupt_assertions(signed_assertions: &mut WotsSignatures, index: usize) {
    let (_, wots_sk) = get_wots_keys();
    corrupt_signed_assertions(&wots_sk, signed_assertions, index);
}   

pub fn validate_assert_bitcom(rpc: &Client, assert_txid: Txid) -> Option<(usize, Script)> {
    fn validate(res: &mut Option<(usize, Script)>, vk: &VerifyingKey, signed_asserts: WotsSignatures, inpubkeys: WotsPublicKeys) {
        *res = validate_assertions(&vk, signed_asserts, inpubkeys);
    }

    let signed_assertions = extract_signed_assertions(rpc, assert_txid);
    let (vk, _, _) = load_proof_from_file(config::PROOF_PATH);
    let (wots_pk, _) = get_wots_keys();
    let mut res = None;
    utils::suppress_output(||{
        validate(&mut res, &vk, signed_assertions, wots_pk);
    });
    res
}

pub fn extract_signed_assertions(rpc: &Client, assert_txid: Txid) -> WotsSignatures {
    let raw_assert_tx = utils::get_raw_tx(&rpc, assert_txid);
    extract_signed_assertions_from_assert_tx(raw_assert_tx)
}

fn get_wots_keys() -> (WotsPublicKeys, WotsSecretKeys) {
    generate_wots_keys_from_secrets(config::WOTS_SECRET)
}

fn get_revealers<'a>(n_of_n_taproot_public_key: &XOnlyPublicKey, bitcom_lock_scripts: &'a Vec<Script>) -> Vec<Revealer<'a>> {
    let mut revealers = Vec::new();
    for i in 0..bitcom_lock_scripts.len() {
        let revealer = Revealer::new(network(), &n_of_n_taproot_public_key, &bitcom_lock_scripts[i]);
        revealers.push(revealer);
    }
    revealers
}