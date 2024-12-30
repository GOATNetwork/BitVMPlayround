pub mod config;
pub mod utils;
pub mod setup;
pub mod transactions;

use std::io::Write;
use std::fs::File;
use clap::{Command, Arg};
use setup::{check_setup, setup_all};

#[tokio::main]
async fn main() {
    let mut cli = Command::new("bitvm_demo")
        .about("\nthis is a demo of bitvm2, here you can:\n*generate chunked tapscripts for groth16 verifier\n*create bitvm2 instance in different scenarios (include take_1, take_2, disprove)\n*query transaction by txid")
        .subcommand(
            Command::new("init")
                .about("\nthis will: compile example circuit, generate tapscripts, generate bitcommitments")
        )
        .subcommand(
            Command::new("run")
                .about("\ncreate bitvm instances & broadcast transactions")
                .subcommand(
                Command::new("take_1")
                        .about("\"take_1\" corresponds to the situation where no challenge occurs, which is the most common situation")
                )
                .subcommand(
                Command::new("take_2")
                        .about("\"take_2\" corresponds to the situation where challenge failed")
                )
                .subcommand(
                Command::new("disprove")
                        .about("\"disprove\" corresponds to a successful challenge, the operator is malicious and got punished")
                        .arg(
                            Arg::new("corrupt_index")
                                .value_name("INDEX")
                                .long("index")
                                .default_value("1")
                                .help(&format!("[Optional]choose an index from 0 to {}, we will generate a malicious bitcommitment for corresponding index",config::N_ASSERTIONS-1))
                                .value_parser(clap::value_parser!(u64).range(0..config::N_ASSERTIONS as u64))
                        )
                )
        )
        .subcommand(
            Command::new("query")
                .about("\nget transaction by txid, display in terminal if output is not specified")
                .arg(
                    Arg::new("txid")
                        .long("txid")
                        .short('t')
                        .required(true)
                        .value_name("TXID")
                        .help("bitcoin txid hex")
                )
                .arg(
                    Arg::new("file_path")
                        .long("output")
                        .short('o')
                        .required(false)
                        .value_name("PATH")
                        .help("[Optional]write output to given file")
                )
        );

    let help = cli.render_long_help();
    match cli.get_matches().subcommand() {
        Some(("init", _)) => {
            setup_all();
        },
        Some(("run", cmd)) => {
            match cmd.subcommand() {
                Some(("take_1", _)) => {
                    let rpc = utils::new_rpc_client().await;

                    println!("\npeg-in......");
                    let (peg_in_txid, peg_in_tx_weight) = transactions::peg_in(&rpc);
                    println!("peg-in txid: {peg_in_txid}, weight:{} WU", peg_in_tx_weight.to_wu());

                    let bitcom_lock_scripts = transactions::get_bitcom_lock_scripts();

                    println!("\nkick_off......");
                    let ((kick_off_1_txid, kickoff_1_tx_weight), (kick_off_2_txid, kickoff_2_tx_weight)) = transactions::kick_off(&rpc, &bitcom_lock_scripts);
                    println!("kick_off_1 txid: {kick_off_1_txid}, weight:{} WU", kickoff_1_tx_weight.to_wu());
                    println!("kick_off_2 txid: {kick_off_2_txid}, weight:{} WU", kickoff_2_tx_weight.to_wu());

                    println!("\ntake_1......");
                    let (take_1_txid, take_1_tx_weight) = transactions::take_1(&rpc, peg_in_txid, kick_off_1_txid, kick_off_2_txid);
                    println!("take_1 txid: {take_1_txid}, weight:{} WU", take_1_tx_weight.to_wu());

                    let total_weight = peg_in_tx_weight.to_wu() + kickoff_1_tx_weight.to_wu() + kickoff_2_tx_weight.to_wu() + take_1_tx_weight.to_wu();
                    let fee_rate = 20;
                    let fee_sat = total_weight * fee_rate / 4;
                    let fee= (fee_sat as f64) / 1_000_000_000.0;
                    println!("\ntotal_cost: {total_weight} WU, estimate_fee: {fee} BTC / {fee_sat} sats (fee_rate: {fee_rate} sats/vB)")
                },
                Some(("take_2", _)) => {
                    if !check_setup() {
                        println!("initialization is not complete, please init first");
                        std::process::exit(2);
                    };
                    let rpc = utils::new_rpc_client().await;

                    println!("\npeg-in......");
                    let (peg_in_txid, peg_in_tx_weight) = transactions::peg_in(&rpc);
                    println!("peg-in txid: {peg_in_txid}, weight:{} WU", peg_in_tx_weight.to_wu());

                    let bitcom_lock_scripts = transactions::get_bitcom_lock_scripts();

                    println!("\nkick_off......");
                    let ((kick_off_1_txid, kickoff_1_tx_weight), (kick_off_2_txid, kickoff_2_tx_weight)) = transactions::kick_off(&rpc, &bitcom_lock_scripts);
                    println!("kick_off_1 txid: {kick_off_1_txid}, weight:{} WU", kickoff_1_tx_weight.to_wu());
                    println!("kick_off_2 txid: {kick_off_2_txid}, weight:{} WU", kickoff_2_tx_weight.to_wu());

                    println!("\nchallenge......");
                    let (challenge_txid, challenge_tx_weight) = transactions::challenge(&rpc, kick_off_1_txid);
                    println!("challenge txid: {challenge_txid}, weight:{} WU", challenge_tx_weight.to_wu());


                    println!("\nassert...... (this may take several minutes)");
                    let connector_c_tapscripts = transactions::get_assert_tapscripts();
                    let ((assert_txid, assert_tx_weight), connector_c_address) = transactions::assert(&rpc, kick_off_2_txid, &bitcom_lock_scripts, &connector_c_tapscripts, None);
                    println!("assert txid: {assert_txid}, weight:{} WU", assert_tx_weight.to_wu());

                    println!("\ntake_2......");
                    let (take_2_txid, take_2_tx_weight) = transactions::take_2(&rpc, peg_in_txid, assert_txid, &connector_c_tapscripts, Some(connector_c_address));
                    println!("take_2 txid: {take_2_txid}, weight:{} WU", take_2_tx_weight.to_wu());

                    let total_weight = peg_in_tx_weight.to_wu() + kickoff_1_tx_weight.to_wu() + kickoff_2_tx_weight.to_wu() + challenge_tx_weight.to_wu() + assert_tx_weight.to_wu() + take_2_tx_weight.to_wu();
                    let fee_rate = 20;
                    let fee_sat = total_weight * fee_rate / 4;
                    let fee= (fee_sat as f64) / 1_000_000_000.0;
                    println!("\ntotal_cost: {total_weight} WU, estimate_fee: {fee} BTC / {fee_sat} sats (fee_rate: {fee_rate} sats/vB)")
                },
                Some(("disprove", sub_cmd)) => {
                    if !check_setup() {
                        println!("initialization is not complete, please init first");
                        std::process::exit(2);
                    };
                    let corrupt_index = *sub_cmd.get_one::<u64>("corrupt_index").unwrap() as usize;
                    let rpc = utils::new_rpc_client().await;

                    println!("\npeg-in......");
                    let (peg_in_txid, peg_in_tx_weight) = transactions::peg_in(&rpc);
                    println!("peg-in txid: {peg_in_txid}, weight:{} WU", peg_in_tx_weight.to_wu());

                    let bitcom_lock_scripts = transactions::get_bitcom_lock_scripts();

                    println!("\nkick_off......");
                    let ((kick_off_1_txid, kickoff_1_tx_weight), (kick_off_2_txid, kickoff_2_tx_weight)) = transactions::kick_off(&rpc, &bitcom_lock_scripts);
                    println!("kick_off_1 txid: {kick_off_1_txid}, weight:{} WU", kickoff_1_tx_weight.to_wu());
                    println!("kick_off_2 txid: {kick_off_2_txid}, weight:{} WU", kickoff_2_tx_weight.to_wu());

                    println!("\nchallenge......");
                    let (challenge_txid, challenge_tx_weight) = transactions::challenge(&rpc, kick_off_1_txid);
                    println!("challenge txid: {challenge_txid}, weight:{} WU", challenge_tx_weight.to_wu());

                    println!("\nfake {corrupt_index}th assertions");
                    println!("assert......  (this may take several minutes)");
                    let connector_c_tapscripts = transactions::get_assert_tapscripts();
                    let ((assert_txid, assert_tx_weight), connector_c_address) = transactions::assert(&rpc, kick_off_2_txid, &bitcom_lock_scripts, &connector_c_tapscripts, Some(corrupt_index));
                    println!("assert txid: {assert_txid}, weight:{} WU", assert_tx_weight.to_wu());

                    println!("\ndisprove......  (this may take several minutes)");
                    let (disprove_txid, disprove_tx_weight) = transactions::disprove(&rpc, assert_txid, &connector_c_tapscripts, Some(connector_c_address));
                    println!("assert txid: {disprove_txid}, weight:{} WU", disprove_tx_weight.to_wu());

                    let total_weight = peg_in_tx_weight.to_wu() + kickoff_1_tx_weight.to_wu() + kickoff_2_tx_weight.to_wu() + challenge_tx_weight.to_wu() + assert_tx_weight.to_wu() + disprove_tx_weight.to_wu();
                    let fee_rate = 20;
                    let fee_sat = total_weight * fee_rate / 4;
                    let fee= (fee_sat as f64) / 1_000_000_000.0;
                    println!("\ntotal_cost: {total_weight} WU, estimate_fee: {fee} BTC / {fee_sat} sats (fee_rate: {fee_rate} sats/vB)")
                },
                _ => {},
            }
        },
        Some(("query", cmd)) => {
            let rpc = utils::new_rpc_client().await;
            let txid_hex = cmd.get_one::<String>("txid").unwrap();
            let txid = match utils::decode_txid(&txid_hex) {
                Ok(v) => v,
                Err(e) => {
                    println!("fail to parse input: {:?}",e);
                    std::process::exit(1);
                },
            };
            let tx = utils::get_raw_tx(&rpc, txid);
            let tx_json = serde_json::to_string_pretty(&tx).unwrap();
            let flag = cmd.get_one::<String>("file_path");
            match flag {
                Some(path) => {
                    let mut file = File::create(path).unwrap();
                    file.write_all(tx_json.as_bytes()).unwrap();
                },
                _ => println!("txid: {:?}\n{}", txid, tx_json),
            }
        },
        _ => { println!("{help}"); },
    }

}