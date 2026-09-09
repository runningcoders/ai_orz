//! FederationContract DAO SQLite 单元测试

use super::*;
use crate::models::federation_contract::FederationContractPo;
use crate::pkg::RequestContext;
use sqlx::SqlitePool;

fn new_ctx(pool: SqlitePool) -> RequestContext {
    crate::pkg::request_context_test_support::new_test_ctx("contract-dao-test", pool)
}

#[sqlx::test(migrations = "./migrations")]
async fn test_insert_and_find_by_pair(pool: SqlitePool) {
    let ctx = new_ctx(pool);
    let dao = new();

    let contract = FederationContractPo::new("org_a".into(), "org_b".into());
    dao.insert(ctx.clone(), &contract).await.unwrap();

    let found = dao
        .find_by_pair(ctx.clone(), "org_a", "org_b")
        .await
        .unwrap()
        .expect("contract should exist");
    assert_eq!(found.id, contract.id);
    assert_eq!(found.state, common::enums::FederationContractState::Active);
    assert!(found.has_capability("a2a_task"));

    // 反向查找不命中（合约有方向：属主 = local_org_id）
    assert!(
        dao.find_by_pair(ctx.clone(), "org_b", "org_a")
            .await
            .unwrap()
            .is_none()
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn test_find_active_filters_terminated(pool: SqlitePool) {
    let ctx = new_ctx(pool);
    let dao = new();

    let mut contract = FederationContractPo::new("org_a".into(), "org_b".into());
    dao.insert(ctx.clone(), &contract).await.unwrap();

    // active 可查
    assert!(
        dao.find_active_by_pair(ctx.clone(), "org_a", "org_b")
            .await
            .unwrap()
            .is_some()
    );

    // 终止后 active 查询不命中，但 find_by_pair 仍可见（审计保留）
    contract.state = common::enums::FederationContractState::Terminated;
    dao.update(ctx.clone(), &contract).await.unwrap();
    assert!(
        dao.find_active_by_pair(ctx.clone(), "org_a", "org_b")
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        dao.find_by_pair(ctx.clone(), "org_a", "org_b")
            .await
            .unwrap()
            .is_some()
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn test_list_by_org_scoped_to_owner(pool: SqlitePool) {
    let ctx = new_ctx(pool);
    let dao = new();

    dao.insert(
        ctx.clone(),
        &FederationContractPo::new("org_a".into(), "org_b".into()),
    )
    .await
    .unwrap();
    dao.insert(
        ctx.clone(),
        &FederationContractPo::new("org_c".into(), "org_a".into()),
    )
    .await
    .unwrap();

    let list = dao.list_by_org(ctx, "org_a").await.unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].peer_org_id, "org_b");
}
