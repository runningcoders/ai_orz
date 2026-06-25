//! FastEmbed 集成测试

#[cfg(test)]
mod tests {
    use super::*;
    use ::fastembed::{TextEmbedding, InitOptions};
use common::bail_err;

    #[tokio::test]
    async fn test_fastembed_basic() {
        // 测试 fastembed 基本功能
        let embedding = TextEmbedding::try_new(InitOptions::default()).unwrap();
        
        let texts = vec![
            "hello world".to_string(),
            "你好世界".to_string(),
        ];
        
        let result = embedding.embed(texts, None).unwrap();
        
        assert_eq!(result.len(), 2);
        assert!(!result[0].is_empty());
        assert!(!result[1].is_empty());
        
        println!("向量维度: {}", result[0].len());
        println!("测试通过！");
    }
}
