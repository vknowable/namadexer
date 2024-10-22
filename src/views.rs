use crate::utils::reverse_checksums;
use std::collections::HashMap;
use once_cell::sync::Lazy;
use strum_macros::EnumIter;

// All view types
#[derive(Eq, Hash, PartialEq, EnumIter)]
pub enum ViewType {
    BecomeValidator,
    Bond,
    BridgePool,
    ChangeConsensusKey,
    ChangeValidatorCommission,
    ChangeValidatorMetadata,
    ClaimRewards,
    DeactivateValidator,
    Ibc,
    InitAccount,
    InitProposal,
    ReactivateValidator,
    Redelegate,
    ResignSteward,
    RevealPk,
    Transfer,
    Unbond,
    UnjailValidator,
    UpdateAccount,
    UpdateStewardCommission,
    VoteProposal,
    Withdraw,
}

// Templates for the create view queries
static QUERY_TEMPLATES: Lazy<HashMap<ViewType, &'static str>> = Lazy::new(|| {
    let mut map = HashMap::new();
    
    map.insert(ViewType::BecomeValidator, 
        "SELECT
        hash AS txid,
        data->>'address' AS address,
        data->>'consensus_key' AS consensus_key, 
        data->>'eth_cold_key' AS eth_cold_key, 
        data->>'eth_hot_key' AS eth_hot_key, 
        data->>'protocol_key' AS protocol_key, 
        data->>'commission_rate' AS commission_rate, 
        data->>'max_commission_rate_change' AS max_commission_rate_change, 
        data->>'email' AS email, 
        data->>'description' AS description, 
        data->>'website' AS website, 
        data->>'discord_handle' AS discord_handle, 
        data->>'avatar' AS avatar
        FROM {network}.inner_transactions WHERE code = '\\x{code}';");

    map.insert(ViewType::Bond, 
        "SELECT
        hash AS txid,
        data->>'validator' AS validator,
        data->>'amount' AS amount,
        data->>'source' AS source
        FROM {network}.inner_transactions WHERE code = '\\x{code}';");

    map.insert(ViewType::BridgePool, 
        "SELECT
        hash AS txid,
        data
        FROM {network}.inner_transactions WHERE code = '\\x{code}';");
    
    map.insert(ViewType::ChangeConsensusKey,
        "SELECT
        hash AS txid,
        data->>'validator' AS validator,
        data->>'consensus_key' AS consensus_key
        FROM {network}.inner_transactions WHERE code = '\\x{code}';");

    map.insert(ViewType::ChangeValidatorCommission,
        "SELECT
        hash AS txid,
        data->>'validator' AS validator,
        data->>'new_rate' AS new_rate
        FROM {network}.inner_transactions WHERE code = '\\x{code}';");

    map.insert(ViewType::ChangeValidatorMetadata,
        "SELECT
        hash AS txid,
        data->>'validator' AS validator,
        data->>'email' AS email,
        data->>'description' AS description,
        data->>'website' AS website,
        data->>'discord_handle' AS discord_handle,
        data->>'avatar' AS avatar,
        data->>'commission_rate' AS commission_rate
        FROM {network}.inner_transactions WHERE code = '\\x{code}';");

    map.insert(ViewType::ClaimRewards,
        "SELECT
        hash AS txid,
        data->>'validator' AS validator,
        data->>'source' AS source
        FROM {network}.inner_transactions WHERE code = '\\x{code}';");

    map.insert(ViewType::DeactivateValidator,
        "SELECT
        hash AS txid,
        data AS address
        FROM {network}.inner_transactions WHERE code = '\\x{code}';");

    map.insert(ViewType::Ibc,
        "SELECT
        hash AS txid,
        data
        FROM {network}.inner_transactions WHERE code = '\\x{code}';");

    map.insert(ViewType::InitAccount,
        "SELECT
        hash AS txid,
        data->>'public_keys' AS public_keys,
        data->>'vp_code_hash' AS vp_code_hash,
        data->>'threshold' AS threshold
        FROM {network}.inner_transactions WHERE code = '\\x{code}';");

    map.insert(ViewType::InitProposal,
        "SELECT
        hash AS txid,
        data->>'id' AS id,
        data->>'content' AS content,
        data->>'author' AS author,
        data->>'r#type' AS rtype,
        data->>'voting_start_epoch' AS voting_start_epoch,
        data->>'voting_end_epoch' AS voting_end_epoch,
        data->>'grace_epoch' AS grace_epoch
        FROM {network}.inner_transactions WHERE code = '\\x{code}';");

    map.insert(ViewType::ReactivateValidator,
        "SELECT
        hash AS txid,
        data AS address
        FROM {network}.inner_transactions WHERE code = '\\x{code}';");

    map.insert(ViewType::Redelegate,
        "SELECT
        hash AS txid,
        data->>'redel_bond_start' AS redel_bond_start,
        data->>'src_validator' AS src_validator,
        data->>'bond_start' AS bond_start,
        data->>'amount' AS amount
        FROM {network}.inner_transactions WHERE code = '\\x{code}';");

    map.insert(ViewType::ResignSteward,
        "SELECT
        hash AS txid,
        data AS address
        FROM {network}.inner_transactions WHERE code = '\\x{code}';");

    map.insert(ViewType::RevealPk,
        "SELECT
        hash AS txid,
        data AS public_key
        FROM {network}.inner_transactions WHERE code = '\\x{code}';");

    map.insert(ViewType::Transfer, 
        "SELECT
        hash AS txid,
        data->'sources'->0->>'owner' AS source,
        data->'targets'->0->>'owner' AS target,
        data->'sources'->0->>'token' AS token,
        data->'sources'->0->>'amount' AS amount,
        data->>'shielded_section_hash' AS shielded,
        memo AS memo
        FROM {network}.inner_transactions WHERE code = '\\x{code}' AND return_code = 0;");

    map.insert(ViewType::Unbond,
        "SELECT
        hash AS txid,
        data->>'validator' AS validator,
        data->>'amount' AS amount,
        data->>'source' AS source
        FROM {network}.inner_transactions WHERE code = '\\x{code}';");

    map.insert(ViewType::UnjailValidator,
        "SELECT
        hash AS txid,
        data AS address
        FROM {network}.inner_transactions WHERE code = '\\x{code}';");

    map.insert(ViewType::UpdateAccount,
        "SELECT
        hash AS txid,
        data->>'addr' AS addr,
        data->>'vp_code_hash' AS vp_code_hash,
        data->>'public_keys' AS public_keys,
        data->>'threshold' AS threshold
        FROM {network}.inner_transactions WHERE code = '\\x{code}';");

    map.insert(ViewType::UpdateStewardCommission,
        "SELECT
        hash AS txid,
        data->>'steward' AS steward,
        data->>'commission' AS commission 
        FROM {network}.inner_transactions WHERE code = '\\x{code}';");

    map.insert(ViewType::VoteProposal,
        "SELECT
        hash AS txid,
        data->>'id' AS id,
        data->>'vote' AS vote,
        data->>'voter' AS voter,
        data->>'delegations' AS delegations
        FROM {network}.inner_transactions WHERE code = '\\x{code}' AND return_code = 0;");

    map.insert(ViewType::Withdraw,
        "SELECT
        hash AS txid,
        data->'validator' AS validator,
        data->'source' AS source
        FROM {network}.inner_transactions WHERE code = '\\x{code}';");

    map
});

fn get_view_name(view_type: &ViewType) -> &str {
    match view_type {
        ViewType::BecomeValidator => "tx_become_validator",
        ViewType::Bond => "tx_bond",
        ViewType::BridgePool => "tx_bridge_pool",
        ViewType::ChangeConsensusKey => "tx_change_consensus_key",
        ViewType::ChangeValidatorCommission => "tx_change_validator_comission",
        ViewType::ChangeValidatorMetadata => "tx_change_validator_metadata",
        ViewType::ClaimRewards => "tx_claim_rewards",
        ViewType::DeactivateValidator => "tx_deactivate_validator",
        ViewType::Ibc => "tx_ibc",
        ViewType::InitAccount => "tx_init_account",
        ViewType::InitProposal => "tx_init_proposal",
        ViewType::ReactivateValidator => "tx_reactivate_validator",
        ViewType::Redelegate => "tx_redelegate",
        ViewType::ResignSteward => "tx_resign_steward",
        ViewType::RevealPk => "tx_reveal_pk",
        ViewType::Transfer => "tx_transfer",
        ViewType::Unbond => "tx_unbond",
        ViewType::UnjailValidator => "tx_unjail_validator",
        ViewType::UpdateAccount => "tx_update_account",
        ViewType::UpdateStewardCommission => "tx_update_steward_commission",
        ViewType::VoteProposal => "tx_vote_proposal",
        ViewType::Withdraw => "tx_withdraw",
    }
}

// Generate a DROP VIEW query
pub fn get_drop_view_query(network: &str, view_type: &ViewType) -> String {
    format!("DROP VIEW IF EXISTS {network}.{};", get_view_name(view_type))
}

// Generate a CREATE VIEW query
pub fn get_create_view_query(network: &str, view_type: &ViewType) -> String {
    let view_name = get_view_name(view_type);

    // Retrieve the query template
    if let Some(query_template) = QUERY_TEMPLATES.get(view_type) {
        if let Some(code) = reverse_checksums().get(view_name) {

            let query_body = query_template
                .replace("{network}", network)
                .replace("{code}", code);

            return format!("CREATE OR REPLACE VIEW {network}.{} AS {}", view_name, query_body);
        }
    }
    
    String::new() // Return empty string if no query template or checksum is found
}