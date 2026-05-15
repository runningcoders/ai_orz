//! Message Channel 配置管理测试
//!
//! 消息渠道配置的 CRUD 测试，属于财务领域

#[cfg(test)]
mod tests {
    use crate::models::message_channel::{MessageChannel, MessageChannelPo, ChannelConfig};
    use crate::pkg::RequestContext;
    use crate::service::domain::finance;
    use common::enums::message_channel::{ChannelStatus, ChannelType};
    use sqlx::SqlitePool;

    fn init_test_env(pool: SqlitePool) -> (std::sync::Arc<dyn finance::FinanceDomain>, RequestContext) {
        // 初始化所有依赖的 DAO
        crate::service::dao::message_channel::init(pool.clone());
        crate::service::dao::model_provider::init(pool.clone());
        crate::service::dao::brain::init();
        crate::service::dao::lark::init(pool.clone());
        crate::service::dao::wechat::init(pool.clone());
        crate::service::dao::slack::init(pool.clone());
        crate::service::dao::email::init(pool.clone());
        crate::service::dao::webhook::init(pool.clone());

        // 初始化 DAL
        crate::service::dal::message_channel::init();
        crate::service::dal::model_provider::init();
        crate::service::dal::brain::init();

        // 创建 Domain
        let domain = finance::new(
            crate::service::dal::model_provider::dal(),
            crate::service::dal::message_channel::dal(),
            crate::service::dal::brain::dal(),
        );

        let ctx = RequestContext::new(
            "test-org-001".to_string(),
            Some("test-user-001".to_string()),
            None,
            None,
        );

        (domain, ctx)
    }

    #[sqlx::test]
    async fn test_channel_crud_operations(pool: SqlitePool) {
        let (domain, ctx) = init_test_env(pool);
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
        let (domain, ctx) = init_test_env(pool);
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
        assert_eq!(results.len(), 3);
    }
}
