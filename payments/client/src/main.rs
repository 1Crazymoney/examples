use {
    anchor_lang::{prelude::Pubkey, solana_program::sysvar, InstructionData},
    anchor_spl::{associated_token, token},
    clockwork_sdk::{
        client::{
            thread_program::{instruction::thread_create, objects::Trigger},
            Client, ClientResult, SplToken,
        },
        PAYER_PUBKEY,
    },
    payments::state::Payment,
    solana_sdk::{
        instruction::{AccountMeta, Instruction},
        native_token::LAMPORTS_PER_SOL,
        signature::Keypair,
        signer::Signer,
        system_program,
        transaction::Transaction,
    },
};

fn main() -> ClientResult<()> {
    // Create Client
    let payer = Keypair::new();
    #[cfg(feature = "devnet")]
    let client = Client::new(payer, "https://api.devnet.solana.com".into());
    #[cfg(not(feature = "devnet"))]
    let client = Client::new(payer, "http://localhost:8899".into());
    client.airdrop(&client.payer_pubkey(), 2 * LAMPORTS_PER_SOL)?;

    // create token mint
    let mint_pubkey = client
        .create_token_mint(&client.payer_pubkey(), 9)
        .unwrap()
        .pubkey();

    // Derive PDAs
    let recipient_pubkey = Keypair::new().pubkey();
    let payment_pubkey = Payment::pubkey(client.payer_pubkey(), mint_pubkey, recipient_pubkey);
    let thread_pubkey = clockwork_sdk::thread_program::accounts::Thread::pubkey(
        client.payer_pubkey(),
        "payment".into(),
    );

    // airdrop to payment thread
    client.airdrop(&thread_pubkey, LAMPORTS_PER_SOL)?;

    // Create authority token account
    let authority_token_account = client
        .create_token_account(&client.payer_pubkey(), &mint_pubkey)
        .unwrap()
        .pubkey();

    // Get recipient's ATA
    let recipient_ata_pubkey =
        anchor_spl::associated_token::get_associated_token_address(&recipient_pubkey, &mint_pubkey);

    // Mint to authority's ATA
    client.mint_to(
        &client.payer(),
        &mint_pubkey,
        &authority_token_account,
        LAMPORTS_PER_SOL,
        9,
    )?;

    create_payment(
        &client,
        authority_token_account,
        mint_pubkey,
        payment_pubkey,
        thread_pubkey,
        recipient_pubkey,
        recipient_ata_pubkey,
    )?;

    // wait 10 seconds to update payment
    println!("wait 10 seconds to update payment");
    for n in 0..10 {
        println!("{}", n);
        std::thread::sleep(std::time::Duration::from_secs(1));
    }

    update_payment(&client, payment_pubkey)?;

    Ok(())
}

fn create_payment(
    client: &Client,
    authority_token_account: Pubkey,
    mint: Pubkey,
    payment: Pubkey,
    thread: Pubkey,
    recipient: Pubkey,
    recipient_ata_pubkey: Pubkey,
) -> ClientResult<()> {
    // create ix
    let create_payment_ix = Instruction {
        program_id: payments::ID,
        accounts: vec![
            AccountMeta::new_readonly(associated_token::ID, false),
            AccountMeta::new(client.payer_pubkey(), true),
            AccountMeta::new(authority_token_account, false),
            AccountMeta::new_readonly(mint, false),
            AccountMeta::new(payment, false),
            AccountMeta::new_readonly(recipient, false),
            AccountMeta::new_readonly(sysvar::rent::ID, false),
            AccountMeta::new_readonly(system_program::ID, false),
            AccountMeta::new_readonly(token::ID, false),
        ],
        data: payments::instruction::CreatePayment { amount: 10_000 }.data(),
    };

    let distribute_payment_ix = Instruction {
        program_id: payments::ID,
        accounts: vec![
            AccountMeta::new_readonly(associated_token::ID, false),
            AccountMeta::new_readonly(client.payer_pubkey(), false),
            AccountMeta::new(authority_token_account, false),
            AccountMeta::new_readonly(mint, false),
            AccountMeta::new(PAYER_PUBKEY, true),
            AccountMeta::new(payment, false),
            AccountMeta::new(thread, true),
            AccountMeta::new_readonly(recipient, false),
            AccountMeta::new(recipient_ata_pubkey, false),
            AccountMeta::new_readonly(sysvar::rent::ID, false),
            AccountMeta::new_readonly(system_program::ID, false),
            AccountMeta::new_readonly(token::ID, false),
        ],
        data: payments::instruction::DisbursePayment.data(),
    };

    let thread_create = thread_create(
        client.payer_pubkey(),
        "payment".into(),
        distribute_payment_ix.into(),
        client.payer_pubkey(),
        thread,
        Trigger::Cron {
            schedule: "*/2 * * * * * *".into(),
            skippable: true,
        },
    );

    print_explorer_link(thread, "thread".into())?;
    print_explorer_link(authority_token_account, "authority_token_account".into())?;
    print_explorer_link(recipient_ata_pubkey, "recipient_ata_pubkey".into())?;

    sign_send_and_confirm_tx(
        client,
        [create_payment_ix, thread_create].to_vec(),
        None,
        "create_payment and create_thread".to_string(),
    )?;

    Ok(())
}

fn update_payment(client: &Client, payment_pubkey: Pubkey) -> ClientResult<()> {
    let update_payment_ix = Instruction {
        program_id: payments::ID,
        accounts: vec![
            AccountMeta::new(client.payer_pubkey(), true),
            AccountMeta::new(payment_pubkey, false),
        ],
        data: payments::instruction::UpdatePayment {
            amount: Some(50_000),
        }
        .data(),
    };

    sign_send_and_confirm_tx(
        client,
        [update_payment_ix].to_vec(),
        None,
        "update_payment".to_string(),
    )?;

    Ok(())
}

pub fn print_explorer_link(address: Pubkey, label: String) -> ClientResult<()> {
    println!(
        "{}: https://explorer.solana.com/address/{}?cluster=custom",
        label.to_string(),
        address
    );

    Ok(())
}

pub fn sign_send_and_confirm_tx(
    client: &Client,
    ix: Vec<Instruction>,
    signers: Option<Vec<&Keypair>>,
    label: String,
) -> ClientResult<()> {
    let mut tx;

    match signers {
        Some(signer_keypairs) => {
            tx = Transaction::new_signed_with_payer(
                &ix,
                Some(&client.payer_pubkey()),
                &signer_keypairs,
                client.get_latest_blockhash().unwrap(),
            );
        }
        None => {
            tx = Transaction::new_with_payer(&ix, Some(&client.payer_pubkey()));
        }
    }

    tx.sign(&[client.payer()], client.latest_blockhash().unwrap());

    // Send and confirm initialize tx
    match client.send_and_confirm_transaction(&tx) {
        Ok(sig) => println!(
            "{} tx: ✅ https://explorer.solana.com/tx/{}?cluster=custom",
            label, sig
        ),
        Err(err) => println!("{} tx: ❌ {:#?}", label, err),
    }
    Ok(())
}
