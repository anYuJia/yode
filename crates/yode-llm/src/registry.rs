use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use crate::provider::LlmProvider;

pub struct ProviderRegistry {
    providers: RwLock<HashMap<String, Arc<dyn LlmProvider>>>,
}

impl ProviderRegistry {
    pub fn new() -> Self {
        Self {
            providers: RwLock::new(HashMap::new()),
        }
    }

    pub fn register(&self, provider: Arc<dyn LlmProvider>) {
        let name = provider.name().to_string();
        self.providers.write().unwrap().insert(name, provider);
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn LlmProvider>> {
        self.providers.read().unwrap().get(name).cloned()
    }

    pub fn list(&self) -> Vec<String> {
        self.providers.read().unwrap().keys().cloned().collect()
    }
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}
