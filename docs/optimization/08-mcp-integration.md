# MCP 集成架构深度分析与优化建议

## 1. Claude Code MCP 架构

### 1.1 MCP 目录结构

Claude Code 的 MCP 实现位于 `src/services/mcp/`：

```
src/services/mcp/
├── MCPConnectionManager.tsx    # 连接管理器
├── client.ts                   # MCP 客户端
├── config.ts                   # 配置管理
├── auth.ts                     # OAuth 认证
├── elicitationHandler.ts       # 请求处理
├── envExpansion.ts             # 环境变量展开
├── headersHelper.ts            # 请求头辅助
├── normalization.ts            # 标准化
├── oauthPort.ts                # OAuth 端口管理
├── officialRegistry.ts         # 官方注册表
├── types.ts                    # 类型定义
├── useManageMCPConnections.ts  # React Hook
├── utils.ts                    # 工具函数
├── vscoeSdkMcp.ts              # VSCode SDK
└── xaa.ts / xaaIdpLogin.ts     # XAA 认证
```

### 1.2 MCP 类型定义

```typescript
// src/services/mcp/types.ts

import type { Client } from '@modelcontextprotocol/sdk/client/index.js'

/**
 * MCP 服务器配置
 */
export type MCPServerConfig = {
  name: string;
  command: string;
  args?: string[];
  env?: Record<string, string>;
  cwd?: string;
  
  // SSE 传输配置
  url?: string;
  
  // 认证配置
  auth?: MCPOAuthConfig;
  
  // 超时配置
  timeout?: number;
};

/**
 * OAuth 配置
 */
export type MCPOAuthConfig = {
  issuer?: string;
  authorizationUrl?: string;
  tokenUrl?: string;
  scopes?: string[];
};

/**
 * MCP 服务器连接
 */
export type MCPServerConnection = {
  config: MCPServerConfig;
  client: Client | null;
  status: 'disconnected' | 'connecting' | 'connected' | 'error';
  error?: string;
  tools?: MCPTool[];
  resources?: ServerResource[];
  prompts?: MCPPrompt[];
};

/**
 * MCP 工具
 */
export type MCPTool = {
  name: string;
  description: string;
  inputSchema: Record<string, unknown>;
  serverName: string;
};

/**
 * MCP 资源
 */
export type ServerResource = {
  uri: string;
  name: string;
  description?: string;
  mimeType?: string;
  serverName: string;
};

/**
 * MCP Prompt
 */
export type MCPPrompt = {
  name: string;
  description?: string;
  arguments?: MCPPromptArgument[];
  serverName: string;
};

export type MCPPromptArgument = {
  name: string;
  description?: string;
  required?: boolean;
};
```

### 1.3 MCP 连接管理器

```typescript
// src/services/mcp/MCPConnectionManager.tsx

import { Client } from '@modelcontextprotocol/sdk/client/index.js'
import { StdioClientTransport } from '@modelcontextprotocol/sdk/client/stdio.js'
import { SSEClientTransport } from '@modelcontextprotocol/sdk/client/sse.js'

export class MCPConnectionManager {
  private connections: Map<string, MCPServerConnection> = new Map()
  private clients: Map<string, Client> = new Map()
  
  /**
   * 连接到 MCP 服务器
   */
  async connect(config: MCPServerConfig): Promise<void> {
    const connection: MCPServerConnection = {
      config,
      client: null,
      status: 'connecting',
    }
    
    this.connections.set(config.name, connection)
    
    try {
      // 创建传输层
      const transport = this.createTransport(config)
      
      // 创建客户端
      const client = new Client({
        name: 'claude-code',
        version: '1.0.0',
      })
      
      // 连接到服务器
      await client.connect(transport)
      
      // 获取服务器能力
      const tools = await client.listTools()
      const resources = await client.listResources()
      const prompts = await client.listPrompts()
      
      // 更新连接状态
      connection.client = client
      connection.status = 'connected'
      connection.tools = tools.tools as MCPTool[]
      connection.resources = resources.resources as ServerResource[]
      connection.prompts = prompts.prompts as MCPPrompt[]
      
      this.clients.set(config.name, client)
      this.connections.set(config.name, connection)
      
    } catch (error) {
      connection.status = 'error'
      connection.error = error.message
      this.connections.set(config.name, connection)
      throw error
    }
  }
  
  /**
   * 断开连接
   */
  async disconnect(serverName: string): Promise<void> {
    const connection = this.connections.get(serverName)
    if (!connection) return
    
    if (connection.client) {
      await connection.client.close()
    }
    
    connection.status = 'disconnected'
    connection.client = null
    
    this.clients.delete(serverName)
    this.connections.set(serverName, connection)
  }
  
  /**
   * 创建传输层
   */
  private createTransport(config: MCPServerConfig) {
    if (config.url) {
      // SSE 传输
      return new SSEClientTransport(new URL(config.url))
    } else {
      // Stdio 传输
      return new StdioClientTransport({
        command: config.command,
        args: config.args || [],
        env: {
          ...process.env,
          ...config.env,
        },
        cwd: config.cwd,
      })
    }
  }
  
  /**
   * 调用工具
   */
  async callTool(
    serverName: string,
    toolName: string,
    args: Record<string, unknown>,
  ): Promise<MCPToolResult> {
    const client = this.clients.get(serverName)
    
    if (!client) {
      throw new Error(`Not connected to server: ${serverName}`)
    }
    
    return client.callTool({
      name: toolName,
      arguments: args,
    })
  }
  
  /**
   * 读取资源
   */
  async readResource(
    serverName: string,
    uri: string,
  ): Promise<ServerResourceContent> {
    const client = this.clients.get(serverName)
    
    if (!client) {
      throw new Error(`Not connected to server: ${serverName}`)
    }
    
    return client.readResource({ uri })
  }
  
  /**
   * 获取所有连接的工具
   */
  getAllTools(): MCPTool[] {
    const allTools: MCPTool[] = []
    
    for (const [name, connection] of this.connections) {
      if (connection.status === 'connected' && connection.tools) {
        for (const tool of connection.tools) {
          allTools.push({
            ...tool,
            serverName: name,
          })
        }
      }
    }
    
    return allTools
  }
  
  /**
   * 获取所有资源
   */
  getAllResources(): ServerResource[] {
    const allResources: ServerResource[] = []
    
    for (const [name, connection] of this.connections) {
      if (connection.status === 'connected' && connection.resources) {
        for (const resource of connection.resources) {
          allResources.push({
            ...resource,
            serverName: name,
          })
        }
      }
    }
    
    return allResources
  }
}
```

### 1.4 MCP 配置管理

```typescript
// src/services/mcp/config.ts

import { homedir } from 'os'
import { join } from 'path'
import { readFile, writeFile } from 'fs/promises'

const MCP_CONFIG_PATH = join(homedir(), '.claude', 'mcp.json')

export type MCPConfig = {
  servers: Record<string, MCPServerConfig>
}

export async function loadMCPConfig(): Promise<MCPConfig> {
  try {
    const content = await readFile(MCP_CONFIG_PATH, 'utf-8')
    return JSON.parse(content)
  } catch (error) {
    if (error.code === 'ENOENT') {
      return { servers: {} }
    }
    throw error
  }
}

export async function saveMCPConfig(config: MCPConfig): Promise<void> {
  await writeFile(MCP_CONFIG_PATH, JSON.stringify(config, null, 2))
}

export async function addMCPServer(
  name: string,
  config: MCPServerConfig,
): Promise<void> {
  const current = await loadMCPConfig()
  current.servers[name] = config
  await saveMCPConfig(current)
}

export async function removeMCPServer(name: string): Promise<void> {
  const current = await loadMCPConfig()
  delete current.servers[name]
  await saveMCPConfig(current)
}
```

### 1.5 MCP React Hook

```typescript
// src/services/mcp/useManageMCPConnections.ts

import { useEffect, useState } from 'react'
import { MCPConnectionManager, MCPServerConfig } from './MCPConnectionManager'

export function useManageMCPConnections() {
  const [connections, setConnections] = useState<
    Map<string, MCPServerConnection>
  >(new Map())
  
  const [manager] = useState(() => new MCPConnectionManager())
  
  // 加载配置并连接
  useEffect(() => {
    async function load() {
      const config = await loadMCPConfig()
      
      for (const [name, serverConfig] of Object.entries(config.servers)) {
        try {
          await manager.connect(serverConfig)
        } catch (error) {
          console.error(`Failed to connect to ${name}:`, error)
        }
      }
      
      setConnections(manager.connections)
    }
    
    load()
    
    return () => {
      // 清理连接
      for (const name of manager.connections.keys()) {
        manager.disconnect(name)
      }
    }
  }, [])
  
  const connectServer = async (config: MCPServerConfig) => {
    await manager.connect(config)
    setConnections(new Map(manager.connections))
  }
  
  const disconnectServer = async (name: string) => {
    await manager.disconnect(name)
    setConnections(new Map(manager.connections))
  }
  
  const callTool = async (
    serverName: string,
    toolName: string,
    args: Record<string, unknown>,
  ) => {
    return manager.callTool(serverName, toolName, args)
  }
  
  return {
    connections,
    connectServer,
    disconnectServer,
    callTool,
    getAllTools: () => manager.getAllTools(),
    getAllResources: () => manager.getAllResources(),
  }
}
```

### 1.6 MCP OAuth 认证

```typescript
// src/services/mcp/auth.ts

import { OAuthClient } from '@modelcontextprotocol/sdk/client/oauth.js'

export class MCPOAuthProvider {
  private clients: Map<string, OAuthClient> = new Map()
  
  async authenticate(
    serverName: string,
    config: MCPOAuthConfig,
  ): Promise<string> {
    const client = new OAuthClient({
      issuer: config.issuer,
      authorizationUrl: config.authorizationUrl,
      tokenUrl: config.tokenUrl,
    })
    
    // 启动 OAuth 流程
    const authUrl = await client.authorize(config.scopes || [])
    
    // 用户在浏览器中完成认证
    // ...
    
    // 获取访问令牌
    const token = await client.getToken()
    
    this.clients.set(serverName, client)
    
    return token
  }
  
  async getAccessToken(serverName: string): Promise<string | null> {
    const client = this.clients.get(serverName)
    
    if (!client) {
      return null
    }
    
    return client.getAccessToken()
  }
  
  async refreshToken(serverName: string): Promise<void> {
    const client = this.clients.get(serverName)
    
    if (!client) {
      throw new Error(`Not authenticated to server: ${serverName}`)
    }
    
    await client.refreshToken()
  }
}
```

---

## 2. Yode 当前 MCP 分析

### 2.1 当前实现

Yode 的 MCP 实现位于 `crates/yode-mcp/`：

```
crates/yode-mcp/
├── src/
│   ├── client.rs       # MCP 客户端
│   ├── config.rs       # 配置管理
│   ├── lib.rs          # 库入口
│   ├── registry.rs     # MCP 注册表
│   └── transport/
│       ├── mod.rs
│       ├── stdio.rs    # Stdio 传输
│       └── sse.rs      # SSE 传输
```

### 2.2 当前代码分析

```rust
// crates/yode-mcp/src/client.rs

use rmcp::{Client, RoleClient};
use rmcp::transport::child_process::TokioChildProcess;

pub struct McpClient {
    client: Client<RoleClient>,
    server_name: String,
}

impl McpClient {
    pub async fn connect_stdio(
        name: String,
        command: String,
        args: Vec<String>,
    ) -> Result<Self> {
        let transport = TokioChildProcess::new(
            tokio::process::Command::new(&command)
                .args(&args)
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::inherit())
        )?;
        
        let client = Client::connect(transport).await?;
        
        Ok(Self {
            client,
            server_name: name,
        })
    }
    
    pub async fn list_tools(&self) -> Vec<McpTool> {
        // 实现...
        vec![]
    }
    
    pub async fn call_tool(
        &self,
        tool_name: &str,
        args: serde_json::Value,
    ) -> Result<McpToolResult> {
        // 实现...
        Ok(McpToolResult { content: vec![] })
    }
}
```

---

## 3. 优化建议

### 3.1 第一阶段：连接管理器

#### 3.1.1 连接管理器实现

```rust
// crates/yode-mcp/src/manager.rs

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use crate::client::McpClient;
use crate::config::McpServerConfig;

/// MCP 服务器连接状态
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ConnectionStatus {
    Disconnected,
    Connecting,
    Connected,
    Error,
}

/// MCP 服务器连接
#[derive(Debug)]
pub struct McpConnection {
    pub config: McpServerConfig,
    pub client: Option<Arc<McpClient>>,
    pub status: ConnectionStatus,
    pub error: Option<String>,
}

/// MCP 连接管理器
pub struct McpConnectionManager {
    connections: RwLock<HashMap<String, McpConnection>>,
}

impl McpConnectionManager {
    pub fn new() -> Self {
        Self {
            connections: RwLock::new(HashMap::new()),
        }
    }
    
    /// 连接到 MCP 服务器
    pub async fn connect(&self, config: McpServerConfig) -> Result<()> {
        let connection = McpConnection {
            config: config.clone(),
            client: None,
            status: ConnectionStatus::Connecting,
            error: None,
        };
        
        {
            let mut conns = self.connections.write().await;
            conns.insert(config.name.clone(), connection);
        }
        
        // 创建连接
        match self.create_connection(&config).await {
            Ok(client) => {
                let mut conns = self.connections.write().await;
                if let Some(conn) = conns.get_mut(&config.name) {
                    conn.client = Some(Arc::new(client));
                    conn.status = ConnectionStatus::Connected;
                }
                Ok(())
            }
            Err(e) => {
                let mut conns = self.connections.write().await;
                if let Some(conn) = conns.get_mut(&config.name) {
                    conn.status = ConnectionStatus::Error;
                    conn.error = Some(e.to_string());
                }
                Err(e)
            }
        }
    }
    
    /// 断开连接
    pub async fn disconnect(&self, server_name: &str) {
        let mut conns = self.connections.write().await;
        
        if let Some(conn) = conns.get_mut(server_name) {
            conn.client = None;
            conn.status = ConnectionStatus::Disconnected;
        }
    }
    
    /// 获取客户端
    pub async fn get_client(&self, server_name: &str) -> Option<Arc<McpClient>> {
        let conns = self.connections.read().await;
        
        conns.get(server_name)
            .and_then(|c| c.client.clone())
    }
    
    /// 获取所有连接的工具
    pub async fn get_all_tools(&self) -> Vec<McpToolWithServer> {
        let conns = self.connections.read().await;
        let mut tools = Vec::new();
        
        for (name, conn) in conns.iter() {
            if conn.status == ConnectionStatus::Connected {
                if let Some(client) = &conn.client {
                    for tool in client.list_tools().await {
                        tools.push(McpToolWithServer {
                            server_name: name.clone(),
                            tool,
                        });
                    }
                }
            }
        }
        
        tools
    }
    
    /// 调用工具
    pub async fn call_tool(
        &self,
        server_name: &str,
        tool_name: &str,
        args: serde_json::Value,
    ) -> Result<McpToolResult> {
        let client = self.get_client(server_name).await
            .ok_or_else(|| anyhow::anyhow!("Not connected to server: {}", server_name))?;
        
        client.call_tool(tool_name, args).await
    }
    
    async fn create_connection(&self, config: &McpServerConfig) -> Result<McpClient> {
        // 根据配置创建连接
        if let Some(url) = &config.url {
            // SSE 连接
            McpClient::connect_sse(config.name.clone(), url.clone()).await
        } else {
            // Stdio 连接
            McpClient::connect_stdio(
                config.name.clone(),
                config.command.clone(),
                config.args.clone().unwrap_or_default(),
            ).await
        }
    }
}

#[derive(Debug, Clone)]
pub struct McpToolWithServer {
    pub server_name: String,
    pub tool: McpTool,
}
```

### 3.2 第二阶段：配置管理

#### 3.2.1 MCP 配置

```rust
// crates/yode-mcp/src/config.rs

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::collections::HashMap;

/// MCP 服务器配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    pub name: String,
    
    /// Stdio 传输配置
    #[serde(flatten)]
    pub transport: McpTransportConfig,
    
    /// 环境变量
    #[serde(default)]
    pub env: HashMap<String, String>,
    
    /// 工作目录
    pub cwd: Option<PathBuf>,
    
    /// 超时（秒）
    pub timeout: Option<u64>,
}

/// 传输配置
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum McpTransportConfig {
    /// Stdio 传输
    Stdio {
        command: String,
        args: Option<Vec<String>>,
    },
    /// SSE 传输
    Sse {
        url: String,
    },
}

impl McpServerConfig {
    pub fn stdio(name: String, command: String, args: Vec<String>) -> Self {
        Self {
            name,
            transport: McpTransportConfig::Stdio {
                command,
                args: Some(args),
            },
            env: HashMap::new(),
            cwd: None,
            timeout: None,
        }
    }
    
    pub fn sse(name: String, url: String) -> Self {
        Self {
            name,
            transport: McpTransportConfig::Sse { url },
            env: HashMap::new(),
            cwd: None,
            timeout: None,
        }
    }
}

/// MCP 配置文件
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct McpConfig {
    #[serde(default)]
    pub servers: HashMap<String, McpServerConfig>,
}

impl McpConfig {
    pub fn load() -> Result<Self, Box<dyn std::error::Error>> {
        let config_path = dirs::home_dir()
            .map(|d| d.join(".yode").join("mcp.json"))
            .ok_or("Could not find home directory")?;
        
        if !config_path.exists() {
            return Ok(Self::default());
        }
        
        let content = std::fs::read_to_string(config_path)?;
        Ok(serde_json::from_str(&content)?)
    }
    
    pub fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        let config_path = dirs::home_dir()
            .map(|d| d.join(".yode").join("mcp.json"))
            .ok_or("Could not find home directory")?;
        
        std::fs::create_dir_all(config_path.parent().unwrap())?;
        std::fs::write(config_path, serde_json::to_string_pretty(self)?)?;
        
        Ok(())
    }
    
    pub fn add_server(&mut self, config: McpServerConfig) {
        self.servers.insert(config.name.clone(), config);
    }
    
    pub fn remove_server(&mut self, name: &str) {
        self.servers.remove(name);
    }
}
```

### 3.3 第三阶段：MCP Commands

#### 3.3.1 /mcp 命令

```rust
// crates/yode-tui/src/commands/mcp.rs

use yode_mcp::manager::McpConnectionManager;
use yode_mcp::config::McpConfig;

pub enum McpSubcommand {
    List,
    Connect { name: String },
    Disconnect { name: String },
    Add { name: String, command: String, args: Vec<String> },
    Remove { name: String },
}

pub async fn execute(
    subcommand: McpSubcommand,
    manager: &McpConnectionManager,
) -> String {
    match subcommand {
        McpSubcommand::List => {
            list_servers(manager).await
        }
        McpSubcommand::Connect { name } => {
            connect_server(&name, manager).await
        }
        McpSubcommand::Disconnect { name } => {
            disconnect_server(&name, manager).await
        }
        McpSubcommand::Add { name, command, args } => {
            add_server(name, command, args).await
        }
        McpSubcommand::Remove { name } => {
            remove_server(name).await
        }
    }
}

async fn list_servers(manager: &McpConnectionManager) -> String {
    let tools = manager.get_all_tools().await;
    
    if tools.is_empty() {
        return "没有连接的 MCP 服务器".to_string();
    }
    
    let mut output = String::new();
    output.push_str("MCP 工具:\n\n");
    
    for tool in tools {
        output.push_str(&format!(
            "  [{}] {}\n    {}\n\n",
            tool.server_name,
            tool.tool.name,
            tool.tool.description,
        ));
    }
    
    output
}

async fn connect_server(name: &str, manager: &McpConnectionManager) -> String {
    // 从配置加载服务器配置
    let config = McpConfig::load().ok();
    
    if let Some(cfg) = config.and_then(|c| c.servers.get(name).cloned()) {
        match manager.connect(cfg).await {
            Ok(_) => format!("已连接到 MCP 服务器：{}", name),
            Err(e) => format!("连接失败：{}", e),
        }
    } else {
        format!("未找到服务器配置：{}\n使用 /mcp add 添加服务器", name)
    }
}

async fn disconnect_server(name: &str, manager: &McpConnectionManager) -> String {
    manager.disconnect(name).await;
    format!("已断开 MCP 服务器：{}", name)
}

async fn add_server(name: String, command: String, args: Vec<String>) -> String {
    let mut config = McpConfig::load().unwrap_or_default();
    
    config.add_server(McpServerConfig::stdio(name.clone(), command, args));
    
    if let Err(e) = config.save() {
        return format!("保存配置失败：{}", e);
    }
    
    format!("已添加 MCP 服务器：{}", name)
}

async fn remove_server(name: String) -> String {
    let mut config = McpConfig::load().unwrap_or_default();
    config.remove_server(&name);
    
    if let Err(e) = config.save() {
        return format!("保存配置失败：{}", e);
    }
    
    format!("已删除 MCP 服务器：{}", name)
}
```

### 3.4 第四阶段：MCP 资源支持

#### 3.4.1 资源管理

```rust
// crates/yode-mcp/src/client.rs

impl McpClient {
    /// 列出资源
    pub async fn list_resources(&self) -> Vec<McpResource> {
        // 实现...
        vec![]
    }
    
    /// 读取资源
    pub async fn read_resource(&self, uri: &str) -> Result<McpResourceContent> {
        // 实现...
        Ok(McpResourceContent {
            uri: uri.to_string(),
            content: vec![],
        })
    }
}

#[derive(Debug, Clone)]
pub struct McpResource {
    pub uri: String,
    pub name: String,
    pub description: Option<String>,
    pub mime_type: Option<String>,
}

#[derive(Debug, Clone)]
pub struct McpResourceContent {
    pub uri: String,
    pub content: Vec<ResourceContentBlock>,
}

#[derive(Debug, Clone)]
pub enum ResourceContentBlock {
    Text { text: String },
    Blob { data: Vec<u8>, mime_type: String },
}
```

---

## 4. 配置文件设计

```json
// ~/.yode/mcp.json

{
  "servers": {
    "filesystem": {
      "name": "filesystem",
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-filesystem", "/home/user/projects"],
      "env": {},
      "timeout": 30
    },
    "github": {
      "name": "github",
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-github"],
      "env": {
        "GITHUB_TOKEN": "${GITHUB_TOKEN}"
      }
    },
    "postgres": {
      "name": "postgres",
      "url": "http://localhost:3000/sse"
    }
  }
}
```

---

## 5. 总结

Claude Code MCP 系统特点：

1. **多传输支持** - Stdio 和 SSE 传输
2. **OAuth 认证** - 内置 OAuth 支持
3. **连接管理** - 集中管理服务器连接
4. **React Hook** - 方便的 UI 集成
5. **资源/Prompts** - 完整的 MCP 能力

Yode 优化建议：
1. 完善连接管理器
2. 增强配置系统
3. 实现 /mcp 命令
4. 支持 MCP 资源读取
5. 添加环境变量展开
