//! Message Channel 配置管理测试
//!
//! 消息渠道配置的 CRUD 测试，属于财务领域

use sqlx::SqlitePool;

#[cfg(test)]
mod tests {
    use crate::models::message_channel::{ChannelConfig, MessageChannel, MessageChannelPo};
    use crate::pkg::RequestContext;
    use crate::service::dao::message_channel::MessageChannelQuery;
    use crate::service::domain::finance;
    use common::enums::message_channel::{ChannelStatus, ChannelType};
    use sqlx::SqlitePool;

    async fn init_test_env(
        pool: SqlitePool,
    ) -> (std::sync::Arc<dyn finance::FinanceDomain>, RequestContext) {
        // 初始化依赖的 DAO（不需要传 pool，DAO 通过 ctx 获取 pool）
        crate::service::dao::message_channel::init();
        crate::service::dao::mcp_server::init();
        crate::service::dao::model_provider::init();
        crate::service::dao::tool::init();
        crate::service::dao::tool_call::init();
        crate::service::dao::cortex::init();
        crate::service::dao::attachment::init();

        // 初始化 DAL
        crate::service::dal::message_channel::init();
        crate::service::dal::mcp_server::init();
        crate::service::dal::mcp_tool::init();
        crate::service::dal::model_provider::init();
        crate::service::dal::tool::init();
        crate::service::dal::brain::init();
        crate::service::dal::attachment::init();

        // 创建 Domain
        let domain = finance::new(
            crate::service::dal::model_provider::dal(),
            crate::service::dal::message_channel::dal(),
            crate::service::dal::mcp_server::dal(),
            crate::service::dal::mcp_tool::dal(),
            crate::service::dal::tool::dal(),
            crate::service::dal::brain::dal(),
            crate::service::dal::attachment::dal(),
        );

        let ctx = crate::pkg::request_context_test_support::new_test_ctx("test-user-001", pool);

        (domain, ctx)
    }

    #[sqlx::test]
    async fn test_channel_crud_operations(pool: SqlitePool) {
        let (domain, ctx) = init_test_env(pool).await;
        let user_id = "test-user-001";
        let channel_id = "channel-001";
        let org_id = "test-org-001";

        // 1. 创建渠道
        let po = MessageChannelPo::new(
            channel_id.to_string(),
            org_id.to_string(),
            user_id.to_string(),
            None, // agent_id
            ChannelType::Webhook,
            "测试 Webhook 渠道".to_string(),
            Some("https://example.com/webhook".to_string()),
            None, // access_token
            None, // secret
            ChannelConfig::default(),
            user_id.to_string(),
        );
        let channel = MessageChannel::from_po(po);

        domain
            .message_channel_manage()
            .create_message_channel(ctx.clone(), &channel)
            .await
            .unwrap();

        // 2. 获取渠道验证创建成功
        let fetched = domain
            .message_channel_manage()
            .get_message_channel(ctx.clone(), channel_id)
            .await
            .unwrap();
        assert!(fetched.is_some());
        let fetched = fetched.unwrap();
        assert_eq!(fetched.po.id, channel_id);
        assert_eq!(fetched.po.channel_name, "测试 Webhook 渠道");
        assert_eq!(fetched.po.status, ChannelStatus::Active);

        // 3. 更新渠道
        let mut updated_channel = fetched.clone();
        updated_channel.po.channel_name = "更新后的渠道名称".to_string();
        updated_channel.po.webhook_url = Some("https://new-url.com/webhook".to_string());
        domain
            .message_channel_manage()
            .update_message_channel(ctx.clone(), &updated_channel)
            .await
            .unwrap();

        // 验证更新
        let fetched_after_update = domain
            .message_channel_manage()
            .get_message_channel(ctx.clone(), channel_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(fetched_after_update.po.channel_name, "更新后的渠道名称");
        assert_eq!(
            fetched_after_update.po.webhook_url,
            Some("https://new-url.com/webhook".to_string())
        );

        // 4. 删除渠道
        domain
            .message_channel_manage()
            .delete_message_channel(ctx.clone(), &updated_channel)
            .await
            .unwrap();

        // 验证删除（软删除，仍然可以查询到，但 deleted_at 有值）
        let fetched_after_delete = domain
            .message_channel_manage()
            .get_message_channel(ctx.clone(), channel_id)
            .await
            .unwrap();
        // 软删除后仍然可以查询到
        assert!(fetched_after_delete.is_some());
    }

    #[sqlx::test]
    async fn test_list_and_query_channels(pool: SqlitePool) {
        let (domain, ctx) = init_test_env(pool).await;
        let user_id = "test-user-002";
        let org_id = "test-org-002";

        // 创建 3 个渠道
        for i in 0..3 {
            let po = MessageChannelPo::new(
                format!("channel-{:03}", i),
                org_id.to_string(),
                user_id.to_string(),
                None,
                ChannelType::Webhook,
                format!("渠道 {}", i),
                Some(format!("https://example.com/webhook/{}", i)),
                None,
                None,
                ChannelConfig::default(),
                user_id.to_string(),
            );
            let channel = MessageChannel::from_po(po);
            domain
                .message_channel_manage()
                .create_message_channel(ctx.clone(), &channel)
                .await
                .unwrap();
        }

        // 列出所有渠道
        let all_channels = domain
            .message_channel_manage()
            .list_message_channels(ctx.clone())
            .await
            .unwrap();
        assert_eq!(all_channels.len(), 3);

        // 通用查询
        use crate::service::dao::message_channel::MessageChannelQuery;
        let query = MessageChannelQuery {
            user_id: Some(user_id.to_string()),
            ..Default::default()
        };
        let results = domain
            .message_channel_manage()
            .query_channels(ctx.clone(), query)
            .await
            .unwrap();
        assert_eq!(results.items.len(), 3);
        assert_eq!(results.total, 3);
    }

    #[sqlx::test]
    async fn test_set_channel_status(pool: SqlitePool) {
        let (domain, ctx) = init_test_env(pool).await;
        let user_id = "test-user-003";
        let org_id = "test-org-003";
        let channel_id = "channel-status-test";

        let po = MessageChannelPo::new(
            channel_id.to_string(),
            org_id.to_string(),
            user_id.to_string(),
            None,
            ChannelType::Email,
            "状态测试渠道".to_string(),
            None,
            None,
            None,
            ChannelConfig::default(),
            user_id.to_string(),
        );
        let mut channel = MessageChannel::from_po(po);
        channel.po.status = ChannelStatus::Disabled;

        domain
            .message_channel_manage()
            .create_message_channel(ctx.clone(), &channel)
            .await
            .unwrap();

        // 设置为 Active（启用）- 使用 get + update 模式
        let mut channel_to_enable = domain
            .message_channel_manage()
            .get_message_channel(ctx.clone(), channel_id)
            .await
            .unwrap()
            .unwrap();
        channel_to_enable.po.status = ChannelStatus::Active;
        domain
            .message_channel_manage()
            .update_message_channel(ctx.clone(), &channel_to_enable)
            .await
            .unwrap();

        // 验证状态变更
        let fetched = domain
            .message_channel_manage()
            .get_message_channel(ctx.clone(), channel_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(fetched.po.status, ChannelStatus::Active);

        // 再设置为 Disabled（禁用）- 使用 get + update 模式
        let mut channel_to_disable = domain
            .message_channel_manage()
            .get_message_channel(ctx.clone(), channel_id)
            .await
            .unwrap()
            .unwrap();
        channel_to_disable.po.status = ChannelStatus::Disabled;
        domain
            .message_channel_manage()
            .update_message_channel(ctx.clone(), &channel_to_disable)
            .await
            .unwrap();

        let fetched_final = domain
            .message_channel_manage()
            .get_message_channel(ctx.clone(), channel_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(fetched_final.po.status, ChannelStatus::Disabled);
    }

    #[sqlx::test]
    async fn test_query_channels(pool: SqlitePool) {
        let (domain, ctx) = init_test_env(pool).await;
        let user_id = "test-user-004";
        let org_id = "test-org-004";

        // 创建不同类型的渠道
        let types = vec![ChannelType::Email, ChannelType::Webhook, ChannelType::Slack];
        for (i, channel_type) in types.iter().enumerate() {
            let po = MessageChannelPo::new(
                format!("query-channel-{}", i),
                org_id.to_string(),
                user_id.to_string(),
                None,
                channel_type.clone(),
                format!("Query Channel {}", i),
                None,
                None,
                None,
                ChannelConfig::default(),
                user_id.to_string(),
            );
            let channel = MessageChannel::from_po(po);
            domain
                .message_channel_manage()
                .create_message_channel(ctx.clone(), &channel)
                .await
                .unwrap();
        }

        // 按用户 ID 查询
        let query = MessageChannelQuery {
            user_id: Some(user_id.to_string()),
            ..Default::default()
        };
        let results = domain
            .message_channel_manage()
            .query_channels(ctx.clone(), query)
            .await
            .unwrap();
        assert_eq!(results.items.len(), 3);
        assert_eq!(results.total, 3);

        // 按类型查询
        let query_by_type = MessageChannelQuery {
            user_id: Some(user_id.to_string()),
            channel_type: Some(ChannelType::Webhook),
            ..Default::default()
        };
        let results_by_type = domain
            .message_channel_manage()
            .query_channels(ctx.clone(), query_by_type)
            .await
            .unwrap();
        assert_eq!(results_by_type.items.len(), 1);
        assert_eq!(results_by_type.total, 1);
        assert_eq!(results_by_type.items[0].po.channel_type, ChannelType::Webhook);
    }
}
