//! 模型管理器模块。
//! 负责在内存中注册和查找各类 TTS Provider，提供 O(1) 的模型切换能力。

use std::collections::HashMap;
use std::sync::Arc;

use vox_core::TtsProvider;

/// 模型管理器。
/// 内部通过 HashMap 将字符串 ID 映射到具体的 TTS Provider 实例。
pub struct ModelManager {
    /// 名称到 Provider 的映射。
    providers: HashMap<String, Arc<dyn TtsProvider>>,
}

impl ModelManager {
    /// 创建一个空的模型管理器实例。
    pub fn new() -> Self {
        Self {
            providers: HashMap::new(),
        }
    }

    /// 注册一个新的 Provider。
    /// 如果名称已存在，将覆盖原有 Provider。
    pub fn register(&mut self, name: impl Into<String>, provider: Arc<dyn TtsProvider>) {
        self.providers.insert(name.into(), provider);
    }

    /// 根据名称获取 Provider。
    /// 返回 `Arc<dyn TtsProvider>`，便于在多线程/多任务中共享。
    pub fn get(&self, name: &str) -> Option<Arc<dyn TtsProvider>> {
        self.providers.get(name).cloned()
    }

    /// 返回当前已注册 Provider 的数量，主要用于调试或监控。
    pub fn len(&self) -> usize {
        self.providers.len()
    }
}

