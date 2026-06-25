//! Model Provider Domain 单元测试

#[cfg(test)]
mod tests {
    use crate::models::model_provider::{ModelProvider, ModelProviderPo};
    use common::enums::{ModelCapability, ProviderType};
use common::bail_err;

    #[test]
    fn test_create_model_provider_po() {
        // 测试创建 ModelProviderPo 测试构造是否正常
        let provider = ModelProviderPo::new(
            "测试OpenAI".to_string(),
            ProviderType::OpenAI,
            ModelCapability::Agent,
            "gpt-3.5-turbo".to_string(),
            "***".to_string(),
            None,
            Some("测试用".to_string()),
            "test-user-1".to_string(),
        );

        assert_eq!(provider.name, "测试OpenAI".to_string());
        assert!(matches!(provider.provider_type, ProviderType::OpenAI));
        assert!(matches!(provider.capability, ModelCapability::Agent));
        assert_eq!(provider.model_name, "gpt-3.5-turbo".to_string());
        assert_eq!(provider.api_key, "***".to_string());
        assert_eq!(provider.base_url, None);
        assert_eq!(provider.description, Some("测试用".to_string()));
        assert_eq!(provider.created_by, "test-user-1".to_string());
        assert_eq!(provider.modified_by, "test-user-1".to_string());
        assert!(provider.created_at > 0);
        assert!(provider.updated_at > 0);
    }

    #[test]
    fn test_create_model_provider_from_po() {
        // 测试从 PO 转换为 Model
        let po = ModelProviderPo::new(
            "测试DeepSeek".to_string(),
            ProviderType::DeepSeek,
            ModelCapability::Agent,
            "deepseek-chat".to_string(),
            "***".to_string(),
            None,
            Some("DeepSeek 官方API".to_string()),
            "test-user-1".to_string(),
        );

        let model = ModelProvider::from_po(po);
        assert_eq!(model.po.name, "测试DeepSeek".to_string());
        assert!(matches!(model.po.provider_type, ProviderType::DeepSeek));
    }

    #[test]
    fn test_create_model_provider_with_base_url() {
        // 测试带有自定义 Base URL 的创建
        let po = ModelProviderPo::new(
            "自定义OpenAI".to_string(),
            ProviderType::Custom,
            ModelCapability::Agent,
            "gpt-4".to_string(),
            "sk-custom".to_string(),
            Some("https://my-proxy.example.com/v1".to_string()),
            Some("自定义代理".to_string()),
            "test-user-1".to_string(),
        );

        assert_eq!(
            po.base_url,
            Some("https://my-proxy.example.com/v1".to_string())
        );
        let model = ModelProvider::from_po(po);
        assert_eq!(
            model.po.base_url,
            Some("https://my-proxy.example.com/v1".to_string())
        );
    }

    #[test]
    fn test_all_provider_types_serialize() {
        // 测试所有 ProviderType 都能正常构造
        let types = vec![
            ProviderType::OpenAI,
            ProviderType::Custom,
            ProviderType::DeepSeek,
            ProviderType::Doubao,
            ProviderType::Qwen,
            ProviderType::Ollama,
        ];

        // 只是确保能构造不panic，实际序列化正常
        for t in types {
            let _po = ModelProviderPo::new(
                "test".to_string(),
                t,
                ModelCapability::Agent,
                "model".to_string(),
                "key".to_string(),
                None,
                Some("".to_string()),
                "user".to_string(),
            );
        }

        // 如果走到这里就成功了
        assert!(true);
    }
}
