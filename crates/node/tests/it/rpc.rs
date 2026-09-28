use reth_rpc_api::Web3ApiClient;
use my_chain_node::{
    context::MyChainDefaultContext,
    version::{
        MY_CHAIN_CLIENT_NAME, MY_CHAIN_CLIENT_VERSION, MY_CHAIN_CLIENT_VERSION_SHA,
    },
};
use my_chain_test_utils::e2e_harness::setup::MyChainTestBuilder;

#[tokio::test]
async fn web3_client_version_reports_my_chain_build_identity() -> eyre::Result<()> {
    my_chain_node::init_version_metadata();

    let (_, nodes, _exec, _env, _spammer) = MyChainTestBuilder::builder()
        .flashblocks(false)
        .build()
        .setup::<MyChainDefaultContext>()
        .await?;

    let client = nodes[0]
        .node
        .rpc_client()
        .ok_or_else(|| eyre::eyre::eyre!("expected HTTP RPC client"))?;

    let version = Web3ApiClient::client_version(&client).await?;

    assert_eq!(version, MY_CHAIN_CLIENT_VERSION);
    assert!(version.starts_with(&format!("{MY_CHAIN_CLIENT_NAME}/")));
    assert!(version.contains(MY_CHAIN_CLIENT_VERSION_SHA));

    Ok(())
}
