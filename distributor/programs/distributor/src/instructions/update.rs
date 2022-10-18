use {
    crate::state::*,
    anchor_lang::{
        prelude::*,
        solana_program::{system_program, sysvar, instruction::Instruction},
    },
    anchor_spl::{
        associated_token::{self, get_associated_token_address}, token::{self, Mint}
    },
    clockwork_sdk::{
        PAYER_PUBKEY, 
        queue_program::{
            self, cpi::accounts::QueueUpdate, 
            QueueProgram, 
            accounts::{
                QueueAccount, Queue, 
                Trigger, QueueSettings
            }}},
};

#[derive(Accounts)]
#[instruction(new_recipient: Option<Pubkey>, mint_amount: Option<u64>, trigger: Option<Trigger>)]
pub struct Update<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,

    #[account(address = queue_program::ID)]
    pub clockwork_program: Program<'info, QueueProgram>,

    #[account(
        mut,
        seeds = [SEED_DISTRIBUTOR, distributor.mint.as_ref(), distributor.authority.as_ref()],
        bump,
        has_one = mint,
        has_one = authority,
    )]
    pub distributor: Account<'info, Distributor>,

    #[account(
        mut, 
        address = distributor_queue.pubkey(),
        constraint = distributor_queue.id.eq("distributor")
     )]
    pub distributor_queue: Account<'info, Queue>,
    
    pub mint: Account<'info, Mint>,

    #[account(address = system_program::ID)]
    pub system_program: Program<'info, System>,
}

pub fn handler<'info>(
    ctx: Context<'_, '_, '_, 'info, Update<'info>>, 
    new_recipient: Option<Pubkey>, 
    mint_amount: Option<u64>, 
    trigger: Option<Trigger>
) -> Result<()> {
     // get accounts
    let clockwork_program = &ctx.accounts.clockwork_program;
    let authority = &ctx.accounts.authority;
    let distributor = &mut ctx.accounts.distributor;
    let distributor_queue = &mut ctx.accounts.distributor_queue;
    let mint = &ctx.accounts.mint;
    let system_program = &ctx.accounts.system_program;

    // get distributor bump
    let bump = *ctx.bumps.get("distributor").unwrap();

    // update mint amount
    if let Some(mint_amount) = mint_amount {
        distributor.mint_amount = mint_amount;
    }

    // update new recipient
    if let Some(new_recipient) = new_recipient {
        distributor.recipient = new_recipient;
        distributor.recipient_token_account = get_associated_token_address(&new_recipient, &distributor.mint);

    }

    // new ix data
    let mint_token_ix = Instruction {
        program_id: crate::ID,
        accounts: vec![
            AccountMeta::new_readonly(associated_token::ID, false),
            AccountMeta::new_readonly(distributor.key(), false),
            AccountMeta::new(distributor_queue.key(), true),
            AccountMeta::new(mint.key(), false),
            AccountMeta::new(PAYER_PUBKEY, true),
            AccountMeta::new_readonly(distributor.recipient, false),
            AccountMeta::new(distributor.recipient_token_account, false),
            AccountMeta::new_readonly(sysvar::rent::ID, false),
            AccountMeta::new_readonly(system_program::ID, false),
            AccountMeta::new_readonly(token::ID, false),

        ],
        data: clockwork_sdk::anchor_sighash("distribute").to_vec()
    };

    // update distributor queue
    clockwork_sdk::queue_program::cpi::queue_update(
    CpiContext::new_with_signer(
    clockwork_program.to_account_info(),
        QueueUpdate {
                    authority: authority.to_account_info(), 
                    queue: distributor_queue.to_account_info(), 
                    system_program: system_program.to_account_info()
                },             
        &[&[SEED_DISTRIBUTOR, distributor.mint.as_ref(), distributor.authority.as_ref(), &[bump]]],
        ),
    QueueSettings {
                kickoff_instruction: Some(mint_token_ix.into()), 
                fee: None, 
                rate_limit: None, 
                trigger
            }
    )?;


    Ok(())
}