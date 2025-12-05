#[cfg(test)]
mod tests {
    use super::MessageStore;
    use tokio;

    #[tokio::test]
    async fn test_memory_store() {
        let mut store = MessageStore::new(None).await;
        store.store_message("id1", "hello").await;
        let val = store.get_message("id1").await;
        assert_eq!(val, Some("hello".to_string()));
    }

    #[tokio::test]
    async fn test_memory_store_overwrite() {
        let mut store = MessageStore::new(None).await;
        store.store_message("id2", "foo").await;
        store.store_message("id2", "bar").await;
        let val = store.get_message("id2").await;
        assert_eq!(val, Some("bar".to_string()));
    }

    // To test Redis, set REDIS_URL env var to a running Redis instance
    #[tokio::test]
    async fn test_redis_store() {
        let redis_url = std::env::var("REDIS_URL").ok();
        if redis_url.is_none() {
            // Skip if no Redis URL
            return;
        }
        let mut store = MessageStore::new(redis_url.as_deref()).await;
        store.store_message("id3", "redis_test").await;
        let val = store.get_message("id3").await;
        assert_eq!(val, Some("redis_test".to_string()));
    }
}
