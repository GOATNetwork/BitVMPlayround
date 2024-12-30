use std::collections::HashMap;
use std::fs::{self, metadata}; 
use std::path::Path;
use std::time::SystemTime;
use bitvm::bridge::groth16;
use bitvm::chunk;
use bitvm::groth16::g16;
use bitvm::treepp::*;
use crate::{config, utils};

pub fn check_setup() -> bool {
    let mut flag = true;
    println!("\nChecking if initialization is complete......");

    // let compile_dir = config::COMPILE_PATH;
    // let index = g16::N_TAPLEAVES - 1;
    // let last_tapnode_path = &format!("{}/tapnode_{index}.json", compile_dir);
    // let already_compiled = match metadata(last_tapnode_path) {
    //     Ok(meta) => {
    //         if meta.len() > 0 {
    //             true
    //         } else {
    //             false
    //         }
    //     },
    //     Err(_) => false,
    // };
    // if already_compiled {
    //     println!("Compile is done :)");
    // } else {
    //     println!("Compile is not done yet :(");
    //     flag = false;
    // }
    
    let tapscript_dir = config::TAPSCRIPT_PATH;
    let index = g16::N_TAPLEAVES - 1;
    let last_tapscript_path = &format!("{}/tapscript_{index}.json", tapscript_dir);
    let already_generated = match metadata(last_tapscript_path) {
        Ok(meta) => {
            if meta.len() > 0 {
                true
            } else {
                false
            }
        },
        Err(_) => false,
    };
    if already_generated {
        println!("Generate tapscripts is done :)");
    } else {
        println!("Generate tapscripts is not done yet :(");
        flag = false;
    };

    let sigs_dir = config::WOTS_SIGNATURE_PATH;
    let index = g16::N_VERIFIER_FQS + g16::N_VERIFIER_HASHES + g16::N_VERIFIER_PUBLIC_INPUTS - 1;
    let last_sig_path = &format!("{}/signed_assertion_{index}.json", sigs_dir);
    let already_signed = match metadata(last_sig_path) {
        Ok(meta) => {
            if meta.len() > 0 {
                true
            } else {
                false
            }
        },
        Err(_) => {
            false
        },
    };
    if already_signed {
        println!("Generate WotsSignature is done :)");
    } else {
        println!("Generate WotsSignature is not done yet :(");
        flag = false;
    };

    flag
}

pub fn setup_all() {
    println!("\ncompiling...... (this may take serveral minutes)");
    let now = SystemTime::now();
    compile();
    let duration = match now.elapsed() {
        Ok(v) => v.as_secs().to_string(),
        Err(_) => "?".to_string(),
    };
    println!("done. [{duration} s]");

    println!("\ngenerating tapscripts...... (this may take serveral minutes)");
    let now = SystemTime::now();
    generate_tapscripts();
    let duration = match now.elapsed() {
        Ok(v) => v.as_secs().to_string(),
        Err(_) => "?".to_string(),
    };
    println!("done. [{duration} s]");

    println!("\ngenerating wots_signatures...... (this may take serveral minutes)");
    let now = SystemTime::now();
    generate_signed_assertions();
    let duration = match now.elapsed() {
        Ok(v) => v.as_secs().to_string(),
        Err(_) => "?".to_string(),
    };
    println!("done. [{duration} s]");
}

pub fn compile() { 
    // create data dir
    let compile_dir = config::COMPILE_PATH;
    if Path::new(compile_dir).exists() {
        let index = g16::N_TAPLEAVES - 1;
        let last_tapnode_path = &format!("{}/tapnode_{index}.json", compile_dir);
        let already_compiled = match metadata(last_tapnode_path) {
            Ok(meta) => {
                if meta.len() > 0 {
                    true
                } else {
                    false
                }
            },
            Err(_) => false,
        };
        if already_compiled {
            return;
        }
    } else {
        fs::create_dir(compile_dir).expect("fail to create data dir");
    }

    // generate & write compile data to file 
    let (vk, _, _) = groth16::load_proof_from_file(config::PROOF_PATH);
    let ops_scripts = chunk::api::api_compile(&vk);
    for i in 0..ops_scripts.len() {
        let mut script_cache = HashMap::new();
        script_cache.insert(i as u32, vec![ops_scripts[i].clone()]);
        chunk::test_utils::write_scripts_to_file(script_cache, &format!("{}/tapnode_{i}.json", compile_dir));
    }
}


pub fn generate_tapscripts() {
    // compile must be done before generate tapscripts
    compile();
    // create data dir
    let tapscript_dir = config::TAPSCRIPT_PATH;
    if Path::new(tapscript_dir).exists() {
        let index = g16::N_TAPLEAVES - 1;
        let last_tapscript_path = &format!("{}/tapscript_{index}.json", tapscript_dir);
        let already_generated = match metadata(last_tapscript_path) {
            Ok(meta) => {
                if meta.len() > 0 {
                    true
                } else {
                    false
                }
            },
            Err(_) => false,
        };
        if already_generated {
            return;
        }
    } else {
        fs::create_dir(tapscript_dir).expect("fail to create data dir");
    }

    // load compile data
    let mut op_scripts = vec![];
    for index in 0..g16::N_TAPLEAVES {
        let read = chunk::test_utils::read_scripts_from_file(&format!("{}/tapnode_{index}.json", config::COMPILE_PATH));
        let read_scr = read.get(&(index as u32)).unwrap();
        assert_eq!(read_scr.len(), 1);
        let tap_node = read_scr[0].clone();
        op_scripts.push(tap_node);
    }
    let ops_scripts: [Script; g16::N_TAPLEAVES] = op_scripts.try_into().unwrap(); 
    
    // generate & write tapscripts to file
    let (wots_pk, _) = groth16::generate_wots_keys_from_secrets(config::WOTS_SECRET);
    let taps = chunk::api::generate_tapscripts(wots_pk, &ops_scripts);
    for i in 0..taps.len() {
        let mut script_cache = HashMap::new();
        script_cache.insert(i as u32, vec![taps[i].clone()]);
        chunk::test_utils::write_scripts_to_file(script_cache, &format!("{}/tapscript_{i}.json", tapscript_dir));
    }
}


pub fn generate_signed_assertions() {
    // create data dir 
    let sigs_dir = config::WOTS_SIGNATURE_PATH;
    if Path::new(sigs_dir).exists() {
        let index = g16::N_VERIFIER_FQS + g16::N_VERIFIER_HASHES + g16::N_VERIFIER_PUBLIC_INPUTS - 1;
        let last_sig_path = &format!("{}/signed_assertion_{index}.json", sigs_dir);
        let already_signed = match metadata(last_sig_path) {
            Ok(meta) => {
                if meta.len() > 0 {
                    true
                } else {
                    false
                }
            },
            Err(_) => false,
        };
        if already_signed {
            return;
        }
    } else {
        fs::create_dir(sigs_dir).expect("fail to create data dir");
    }

    // generate & write wots sigs to file
    let (vk, proof, pubin) = groth16::load_proof_from_file(config::PROOF_PATH);
    let (_, wots_sk) = groth16::generate_wots_keys_from_secrets(config::WOTS_SECRET);
    utils::suppress_output(|| {
        groth16::generate_signed_assertions(proof, pubin, &wots_sk, &vk, true, &sigs_dir);
    });
}

