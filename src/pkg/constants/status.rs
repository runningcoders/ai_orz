//! 状态枚举定义

/// AgentPo 状态枚举（用于软删除）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentPoStatus {
    Deleted = 0,
    Normal = 1,
}

impl AgentPoStatus {
    pub fn from_i32(v: i32) -> Self {
        match v {
            0 => Self::Deleted,
            _ => Self::Normal,
        }
    }

    pub fn to_i32(&self) -> i32 {
        *self as i32
    }
}

impl serde::Serialize for AgentPoStatus {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_i32(self.to_i32())
    }
}

impl<'de> serde::Deserialize<'de> for AgentPoStatus {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let v = i32::deserialize(deserializer)?;
        Ok(Self::from_i32(v))
    }
}

impl Default for AgentPoStatus {
    fn default() -> Self {
        Self::Normal
    }
}

/// ModelProvider 状态枚举（用于软删除）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelProviderStatus {
    Deleted = 0,
    Normal = 1,
}

impl ModelProviderStatus {
    pub fn from_i32(v: i32) -> Self {
        match v {
            0 => Self::Deleted,
            _ => Self::Normal,
        }
    }

    pub fn to_i32(&self) -> i32 {
        *self as i32
    }
}

impl serde::Serialize for ModelProviderStatus {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_i32(self.to_i32())
    }
}

impl<'de> serde::Deserialize<'de> for ModelProviderStatus {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let v = i32::deserialize(deserializer)?;
        Ok(Self::from_i32(v))
    }
}

impl Default for ModelProviderStatus {
    fn default() -> Self {
        Self::Normal
    }
}
