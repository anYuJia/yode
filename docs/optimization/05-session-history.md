# 会话与历史记录系统深度分析与优化建议

## 1. Claude Code 会话历史架构

### 1.1 历史记录存储

Claude Code 的历史记录系统位于 `src/history.ts`，使用 JSONL 格式存储：

```typescript
// src/history.ts

// 历史文件存储位置：~/.claude/history.jsonl
// 每行是一个 JSON 对象

const MAX_HISTORY_ITEMS = 100;  // 最多保留 100 条历史

/**
 * 历史记录条目
 */
type LogEntry = {
  sessionId: string;        // 会话 ID
  timestamp: number;         // Unix 时间戳
  type: 'user' | 'assistant';
  message?: string;          // 用户消息
  toolCalls?: ToolCall[];    // 工具调用
  toolResults?: ToolResult[]; // 工具结果
  model?: string;            // 使用的模型
  tokens?: {                 // Token 使用
    input: number;
    output: number;
  };
};

/**
 * 粘贴内容存储
 * 大段文本存储在外部文件，历史中只保存哈希引用
 */
type StoredPastedContent = {
  id: number;
  type: 'text' | 'image';
  content?: string;           // 小内容内联存储
  contentHash?: string;       // 大内容哈希引用
  mediaType?: string;
  filename?: string;
};
```

### 1.2 历史读取器（反向读取）

```typescript
// src/history.ts - 反向读取实现

export async function* makeHistoryReader(): AsyncGenerator<HistoryEntry> {
  for await (const entry of makeLogEntryReader()) {
    yield await logEntryToHistoryEntry(entry);
  }
}

async function* makeLogEntryReader(): AsyncGenerator<LogEntry> {
  const currentSession = getSessionId();
  
  // 1. 先读取未刷新的内存条目
  for (let i = pendingEntries.length - 1; i >= 0; i--) {
    yield pendingEntries[i]!;
  }
  
  // 2. 从全局历史文件反向读取
  const historyPath = join(getClaudeConfigHomeDir(), 'history.jsonl');
  
  try {
    for await (const line of readLinesReverse(historyPath)) {
      try {
        const entry = deserializeLogEntry(line);
        
        // 跳过已跳过的时间戳（用于删除功能）
        if (entry.sessionId === currentSession && 
            skippedTimestamps.has(entry.timestamp)) {
          continue;
        }
        
        yield entry;
      } catch (error) {
        // 跳过格式错误的行
        logForDebugging(`Failed to parse history line: ${error}`);
      }
    }
  } catch (e: unknown) {
    if (getErrnoCode(e) === 'ENOENT') {
      return;  // 文件不存在，正常返回
    }
    throw e;
  }
}

/**
 * 反向读取文件的生成器
 */
export async function* readLinesReverse(filePath: string): AsyncGenerator<string> {
  const stats = await fs.stat(filePath);
  const fileSize = stats.size;
  
  let buffer = Buffer.alloc(BUFFER_SIZE);
  let position = fileSize;
  let leftover = '';
  
  const fd = await fs.open(filePath, 'r');
  
  try {
    while (position > 0) {
      const readPosition = Math.max(0, position - BUFFER_SIZE);
      const bytesRead = await fd.read(
        buffer,
        0,
        Math.min(BUFFER_SIZE, position),
        readPosition
      );
      
      let chunk = buffer.slice(0, bytesRead).toString('utf-8');
      chunk = leftover + chunk;
      
      const lines = chunk.split('\n');
      leftover = lines[0] || '';
      
      // 反向产出（不包括最后一行，因为它可能不完整）
      for (let i = lines.length - 1; i > 0; i--) {
        if (lines[i]) {
          yield lines[i];
        }
      }
      
      position = readPosition;
    }
    
    // 处理第一行
    if (leftover) {
      yield leftover;
    }
  } finally {
    await fd.close();
  }
}
```

### 1.3 粘贴内容存储

```typescript
// src/utils/pasteStore.ts

/**
 * 粘贴内容存储
 * 大段文本存储在 ~/.claude/pastes/{sessionId}/{hash}
 */

const MAX_INLINE_LENGTH = 1024;  // 内联存储最大长度

export async function storePastedText(
  sessionId: string,
  text: string,
): Promise<string> {
  const hash = hashPastedText(text);
  const pasteDir = getPasteDirectory(sessionId);
  
  await fs.ensureDir(pasteDir);
  
  const pastePath = join(pasteDir, hash);
  await fs.writeFile(pastePath, text, 'utf-8');
  
  return hash;
}

export async function retrievePastedText(
  sessionId: string,
  hash: string,
): Promise<string> {
  const pastePath = join(getPasteDirectory(sessionId), hash);
  return fs.readFile(pastePath, 'utf-8');
}

/**
 * 格式化粘贴引用（显示给用户）
 */
export function formatPastedTextRef(id: number, numLines: number): string {
  if (numLines === 0) {
    return `[Pasted text #${id}]`;
  }
  return `[Pasted text #${id} +${numLines} lines]`;
}

/**
 * 解析粘贴引用
 */
export function parseReferences(input: string): Array<{
  id: number;
  match: string;
  index: number;
}> {
  const referencePattern = 
    /\[(Pasted text|Image|\.\.\.Truncated text) #(\d+)(?: \+\d+ lines)?(\.)*\]/g;
  
  const matches = [...input.matchAll(referencePattern)];
  return matches.map(match => ({
    id: parseInt(match[2]),
    match: match[0],
    index: match.index!,
  })).filter(m => m.id > 0);
}
```

### 1.4 历史写入与同步

```typescript
// src/history.ts

// 内存中的待写入条目
let pendingEntries: LogEntry[] = [];
let flushTimeout: NodeJS.Timeout | null = null;

/**
 * 添加历史条目（延迟写入）
 */
export function appendHistory(entry: LogEntry): void {
  pendingEntries.push(entry);
  
  // 批量写入：500ms 内没有新条目时刷新
  if (flushTimeout) {
    clearTimeout(flushTimeout);
  }
  
  flushTimeout = setTimeout(() => {
    flushPendingEntries();
    flushTimeout = null;
  }, 500);
}

/**
 * 刷新待写入条目到磁盘
 */
async function flushPendingEntries(): Promise<void> {
  if (pendingEntries.length === 0) return;
  
  const historyPath = join(getClaudeConfigHomeDir(), 'history.jsonl');
  
  try {
    // 使用行级锁避免并发写入
    const release = await lock(historyPath);
    
    try {
      const fd = await fs.open(historyPath, 'a');
      
      for (const entry of pendingEntries) {
        const line = jsonStringify(entry) + '\n';
        await fd.write(line);
      }
      
      await fd.close();
    } finally {
      release();
    }
    
    pendingEntries = [];
  } catch (error) {
    logError('Failed to flush history entries', error);
  }
}

/**
 * 从历史中删除最后一个条目（用于撤销）
 */
export async function removeLastFromHistory(
  sessionId: string,
): Promise<boolean> {
  // 记录要跳过的时间戳（软删除）
  const lastEntry = pendingEntries[pendingEntries.length - 1];
  
  if (lastEntry && lastEntry.sessionId === sessionId) {
    skippedTimestamps.add(lastEntry.timestamp);
    pendingEntries.pop();
    return true;
  }
  
  return false;
}
```

### 1.5 会话管理

```typescript
// src/bootstrap/state.ts

// 全局会话状态
let sessionId: string | null = null;
let sessionStartTime: number = 0;

/**
 * 生成新的会话 ID
 */
export function generateSessionId(): string {
  // 格式：{date}_{random}
  const date = new Date().toISOString().slice(0, 10).replace(/-/g, '');
  const random = Math.random().toString(36).slice(2, 8);
  return `${date}_${random}`;
}

/**
 * 获取当前会话 ID（不存在则创建）
 */
export function getSessionId(): string {
  if (!sessionId) {
    sessionId = generateSessionId();
    sessionStartTime = Date.now();
  }
  return sessionId;
}

/**
 * 获取项目根目录
 */
let projectRoot: string | null = null;

export function getProjectRoot(): string {
  if (!projectRoot) {
    projectRoot = findProjectRoot(process.cwd());
  }
  return projectRoot;
}

function findProjectRoot(cwd: string): string {
  let current = cwd;
  
  while (current !== path.dirname(current)) {
    // 检查项目标记文件
    if (
      fs.existsSync(join(current, 'package.json')) ||
      fs.existsSync(join(current, 'Cargo.toml')) ||
      fs.existsSync(join(current, '.git'))
    ) {
      return current;
    }
    current = path.dirname(current);
  }
  
  return cwd;
}
```

---

## 2. Yode 当前会话历史分析

### 2.1 当前实现

Yode 使用 SQLite 存储会话：

```rust
// crates/yode-core/src/db.rs

use rusqlite::{Connection, params};

pub struct Database {
    conn: Connection,
}

impl Database {
    pub fn new(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        
        // 初始化表结构
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS sessions (
                id TEXT PRIMARY KEY,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                working_dir TEXT NOT NULL,
                model TEXT NOT NULL
            );
            
            CREATE TABLE IF NOT EXISTS messages (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id TEXT NOT NULL,
                role TEXT NOT NULL,
                content TEXT,
                tool_calls TEXT,
                created_at TEXT NOT NULL,
                FOREIGN KEY (session_id) REFERENCES sessions(id)
            );"
        )?;
        
        Ok(Self { conn })
    }
    
    pub fn save_session(&self, session: &Session) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO sessions (id, created_at, updated_at, working_dir, model)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                session.id,
                session.created_at,
                session.updated_at,
                session.working_dir,
                session.model,
            ],
        )?;
        Ok(())
    }
    
    pub fn load_session(&self, session_id: &str) -> Result<Option<Session>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, created_at, updated_at, working_dir, model 
             FROM sessions WHERE id = ?1"
        )?;
        
        let session = stmt.query_row(params![session_id], |row| {
            Ok(Session {
                id: row.get(0)?,
                created_at: row.get(1)?,
                updated_at: row.get(2)?,
                working_dir: row.get(3)?,
                model: row.get(4)?,
                messages: Vec::new(),
            })
        });
        
        Ok(session.ok())
    }
}
```

### 2.2 会话结构

```rust
// crates/yode-core/src/session.rs

use uuid::Uuid;
use chrono::{DateTime, Utc};

pub struct Session {
    pub id: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub working_dir: PathBuf,
    pub model: String,
    pub messages: Vec<Message>,
}

impl Session {
    pub fn new(working_dir: PathBuf, model: String) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            working_dir,
            model,
            messages: Vec::new(),
        }
    }
}
```

---

## 3. 优化建议

### 3.1 第一阶段：历史文件格式化存储

#### 3.1.1 JSONL 历史存储

```rust
// crates/yode-core/src/history.rs

use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use serde::{Serialize, Deserialize};

const MAX_HISTORY_ITEMS: usize = 100;
const MAX_INLINE_CONTENT: usize = 1024;

/// 历史条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub session_id: String,
    pub timestamp: u64,  // Unix timestamp (ms)
    pub role: String,    // "user" | "assistant"
    pub message: Option<String>,
    pub tool_calls: Option<Vec<HistoryToolCall>>,
    pub tool_results: Option<Vec<HistoryToolResult>>,
    pub model: Option<String>,
    pub tokens: Option<TokenUsage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryToolResult {
    pub tool_call_id: String,
    pub content: String,
    pub is_error: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input: u32,
    pub output: u32,
}

/// 历史管理器
pub struct HistoryManager {
    /// 历史文件路径 (~/.yode/history.jsonl)
    history_path: PathBuf,
    /// 粘贴内容目录 (~/.yode/pastes/)
    paste_dir: PathBuf,
    /// 当前会话 ID
    current_session_id: String,
    /// 待写入的条目
    pending_entries: Vec<HistoryEntry>,
}

impl HistoryManager {
    pub fn new() -> Result<Self> {
        let config_dir = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".yode");
        
        std::fs::create_dir_all(&config_dir)?;
        
        let history_path = config_dir.join("history.jsonl");
        let paste_dir = config_dir.join("pastes");
        std::fs::create_dir_all(&paste_dir)?;
        
        Ok(Self {
            history_path,
            paste_dir,
            current_session_id: String::new(),
            pending_entries: Vec::new(),
        })
    }
    
    pub fn set_current_session(&mut self, session_id: String) {
        self.current_session_id = session_id;
    }
    
    /// 添加历史条目（延迟写入）
    pub fn append(&mut self, entry: HistoryEntry) {
        self.pending_entries.push(entry);
        
        // 触发延迟写入（实际实现中使用定时器）
        self.schedule_flush();
    }
    
    /// 刷新待写入条目
    pub fn flush(&mut self) -> Result<()> {
        if self.pending_entries.is_empty() {
            return Ok(());
        }
        
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.history_path)?;
        
        for entry in &self.pending_entries {
            let line = serde_json::to_string(entry)? + "\n";
            file.write_all(line.as_bytes())?;
        }
        
        file.sync_all()?;
        self.pending_entries.clear();
        
        // 修剪历史记录
        self.truncate_history()?;
        
        Ok(())
    }
    
    /// 反向迭代历史
    pub fn read_reverse(&self) -> impl Iterator<Item = HistoryEntry> {
        ReverseHistoryIterator::new(&self.history_path, &self.current_session_id)
    }
    
    /// 修剪历史记录（保留最近 N 条）
    fn truncate_history(&self) -> Result<()> {
        // 简单实现：读取所有，保留最新，写回
        let entries: Vec<HistoryEntry> = self.read_all()?;
        
        if entries.len() <= MAX_HISTORY_ITEMS {
            return Ok(());
        }
        
        let keep = entries.into_iter().rev().take(MAX_HISTORY_ITEMS).collect::<Vec<_>>();
        
        // 写回文件
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&self.history_path)?;
        
        for entry in keep {
            let line = serde_json::to_string(&entry)? + "\n";
            writeln!(file, "{}", line)?;
        }
        
        Ok(())
    }
    
    fn read_all(&self) -> Result<Vec<HistoryEntry>> {
        if !self.history_path.exists() {
            return Ok(Vec::new());
        }
        
        let file = File::open(&self.history_path)?;
        let reader = BufReader::new(file);
        
        let mut entries = Vec::new();
        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            if let Ok(entry) = serde_json::from_str(&line) {
                entries.push(entry);
            }
        }
        
        Ok(entries)
    }
    
    fn schedule_flush(&mut self) {
        // 实际实现中使用定时器
        // 这里简化处理
    }
}

/// 反向迭代器
pub struct ReverseHistoryIterator {
    lines: Vec<String>,
    index: usize,
    current_session_id: String,
}

impl ReverseHistoryIterator {
    pub fn new(history_path: &Path, session_id: &str) -> Self {
        let mut lines = Vec::new();
        
        if history_path.exists() {
            let file = File::open(history_path).unwrap();
            let reader = BufReader::new(file);
            
            for line in reader.lines().flatten() {
                lines.push(line);
            }
        }
        
        lines.reverse();
        
        Self {
            lines,
            index: 0,
            current_session_id: session_id.to_string(),
        }
    }
}

impl Iterator for ReverseHistoryIterator {
    type Item = HistoryEntry;
    
    fn next(&mut self) -> Option<Self::Item> {
        while self.index < self.lines.len() {
            let line = &self.lines[self.index];
            self.index += 1;
            
            if let Ok(entry) = serde_json::from_str(line) {
                return Some(entry);
            }
        }
        None
    }
}
```

### 3.2 第二阶段：粘贴内容存储

#### 3.2.1 粘贴存储

```rust
// crates/yode-core/src/history.rs

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

impl HistoryManager {
    /// 存储大段文本
    pub fn store_pasted_text(&self, text: &str) -> Result<PasteReference> {
        let hash = if text.len() > MAX_INLINE_CONTENT {
            // 大内容：存储到文件
            let hash = self.compute_hash(text);
            let paste_path = self.get_paste_path(&hash);
            std::fs::write(paste_path, text)?;
            Some(hash.clone())
        } else {
            None
        };
        
        Ok(PasteReference {
            inline_content: if hash.is_none() { Some(text.to_string()) } else { None },
            content_hash: hash,
            lines: text.lines().count(),
        })
    }
    
    /// 检索粘贴内容
    pub fn retrieve_pasted_text(&self, hash: &str) -> Result<String> {
        let paste_path = self.get_paste_path(hash);
        std::fs::read_to_string(paste_path)
            .map_err(|e| anyhow::anyhow!("Failed to retrieve pasted text: {}", e))
    }
    
    fn compute_hash(&self, text: &str) -> String {
        let mut hasher = DefaultHasher::new();
        text.hash(&mut hasher);
        format!("{:x}", hasher.finish())
    }
    
    fn get_paste_path(&self, hash: &str) -> PathBuf {
        let session_dir = self.paste_dir.join(&self.current_session_id);
        std::fs::create_dir_all(&session_dir).ok();
        session_dir.join(hash)
    }
}

/// 粘贴引用
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PasteReference {
    pub inline_content: Option<String>,
    pub content_hash: Option<String>,
    pub lines: usize,
}

impl PasteReference {
    pub fn format_ref(&self, id: usize) -> String {
        if self.lines == 0 {
            format!("[Pasted text #{}]", id)
        } else {
            format!("[Pasted text #{} +{} lines]", id, self.lines)
        }
    }
    
    pub fn get_content(&self, manager: &HistoryManager) -> Result<String> {
        if let Some(ref content) = self.inline_content {
            Ok(content.clone())
        } else if let Some(ref hash) = self.content_hash {
            manager.retrieve_pasted_text(hash)
        } else {
            Err(anyhow::anyhow!("No content available"))
        }
    }
}
```

### 3.3 第三阶段：会话恢复增强

#### 3.3.1 会话摘要

```rust
// crates/yode-core/src/session.rs

/// 会话摘要（用于快速预览）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    pub id: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub working_dir: String,
    pub model: String,
    pub message_count: usize,
    pub first_message_preview: String,
    pub total_tokens: TokenUsage,
    pub estimated_cost: Option<f64>,
}

impl SessionSummary {
    pub fn from_session(session: &Session, db: &Database) -> Result<Self> {
        let first_message = session.messages.first()
            .and_then(|m| m.content.as_ref())
            .map(|c| {
                if c.len() > 100 {
                    format!("{}...", &c[..100])
                } else {
                    c.clone()
                }
            })
            .unwrap_or_else(|| "(no messages)".to_string());
        
        let total_tokens = session.messages.iter()
            .filter_map(|m| m.usage.as_ref())
            .fold(TokenUsage { input: 0, output: 0 }, |acc, u| {
                TokenUsage {
                    input: acc.input + u.prompt_tokens,
                    output: acc.output + u.completion_tokens,
                }
            });
        
        Ok(Self {
            id: session.id.clone(),
            created_at: session.created_at,
            updated_at: session.updated_at,
            working_dir: session.working_dir.display().to_string(),
            model: session.model.clone(),
            message_count: session.messages.len(),
            first_message_preview: first_message,
            total_tokens,
            estimated_cost: None,  // 可以计算
        })
    }
}

impl Database {
    /// 列出最近会话
    pub fn list_recent_sessions(&self, limit: usize) -> Result<Vec<SessionSummary>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, created_at, updated_at, working_dir, model 
             FROM sessions 
             ORDER BY updated_at DESC 
             LIMIT ?1"
        )?;
        
        let sessions: Vec<Session> = stmt
            .query_map(params![limit], |row| {
                Ok(Session {
                    id: row.get(0)?,
                    created_at: row.get(1)?,
                    updated_at: row.get(2)?,
                    working_dir: row.get(3)?,
                    model: row.get(4)?,
                    messages: Vec::new(),
                })
            })?
            .filter_map(|r| r.ok())
            .collect();
        
        // 为每个会话加载消息
        let mut summaries = Vec::new();
        for session in sessions {
            let mut session_with_messages = session.clone();
            session_with_messages.messages = self.load_messages(&session.id)?;
            
            if let Ok(summary) = SessionSummary::from_session(&session_with_messages, self) {
                summaries.push(summary);
            }
        }
        
        Ok(summaries)
    }
    
    /// 加载会话消息
    pub fn load_messages(&self, session_id: &str) -> Result<Vec<Message>> {
        let mut stmt = self.conn.prepare(
            "SELECT content, role, tool_calls, created_at 
             FROM messages 
             WHERE session_id = ?1 
             ORDER BY created_at ASC"
        )?;
        
        let messages = stmt
            .query_map(params![session_id], |row| {
                Ok(Message {
                    content: row.get(0)?,
                    role: row.get(1)?,
                    tool_calls: row.get(2)?,
                    ..Default::default()
                })
            })?
            .filter_map(|r| r.ok())
            .collect();
        
        Ok(messages)
    }
}
```

### 3.4 第四阶段：/sessions 命令

```rust
// crates/yode-tui/src/commands/sessions.rs

use yode_core::session::SessionSummary;

pub fn list_sessions(summaries: Vec<SessionSummary>) -> String {
    if summaries.is_empty() {
        return "没有历史会话".to_string();
    }
    
    let mut output = String::new();
    output.push_str("最近会话:\n\n");
    
    for (i, summary) in summaries.iter().enumerate().take(10) {
        let time_ago = format_duration_ago(&summary.updated_at);
        
        output.push_str(&format!(
            "{}. [{}]\n",
            i + 1,
            summary.id
        ));
        output.push_str(&format!(
            "   时间：{}\n",
            time_ago
        ));
        output.push_str(&format!(
            "   消息：{} | 模型：{}\n",
            summary.message_count,
            summary.model
        ));
        output.push_str(&format!(
            "   预览：{}\n",
            summary.first_message_preview
        ));
        output.push_str("\n");
    }
    
    output.push_str("使用 /resume <session-id> 恢复会话\n");
    
    output
}

fn format_duration_ago(dt: &DateTime<Utc>) -> String {
    let now = Utc::now();
    let duration = now.signed_duration_since(*dt);
    
    if duration.num_seconds() < 60 {
        format!("{}秒前", duration.num_seconds())
    } else if duration.num_minutes() < 60 {
        format!("{}分钟前", duration.num_minutes())
    } else if duration.num_hours() < 24 {
        format!("{}小时前", duration.num_hours())
    } else {
        format!("{}天前", duration.num_days())
    }
}
```

---

## 4. 配置文件设计

```toml
# ~/.config/yode/config.toml

[history]
# 最大历史条目数
max_items = 100

# 粘贴内容阈值（超过此值存储到文件）
paste_inline_threshold = 1024

# 历史文件路径
history_file = "~/.yode/history.jsonl"

# 粘贴存储目录
paste_dir = "~/.yode/pastes"

# 会话恢复
[history.resume]
# 自动恢复上次会话
auto_resume = false

# 显示最近 N 个会话
show_recent_count = 10
```

---

## 5. 总结

Claude Code 历史系统特点：

1. **JSONL 格式** - 易于读取和调试
2. **反向读取** - 高效获取最近历史
3. **粘贴存储** - 大内容外部存储
4. **软删除** - 时间戳标记删除
5. **批量写入** - 延迟写入优化

Yode 使用 SQLite 存储，建议增强：
1. JSONL 历史文件（用于快速预览）
2. 粘贴内容外部存储
3. 会话摘要生成
4. 历史会话列表命令
