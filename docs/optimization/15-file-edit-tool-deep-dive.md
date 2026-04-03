# FileEditTool 深度分析与优化

## 1. FileEditTool 架构概述

Claude Code 的 FileEditTool (`src/tools/FileEditTool/FileEditTool.ts`) 是一个 600+ 行的复杂工具，包含 14 步验证流程，确保文件编辑的安全性和正确性。

### 1.1 工具输入输出 Schema

```typescript
// 输入 Schema
const inputSchema = z.strictObject({
  file_path: z.string().describe('The absolute path to the file to edit'),
  old_string: z.string().describe('The text to replace'),
  new_string: z.string().describe('The text to replace it with'),
  replace_all: z.boolean().optional().describe('Replace all occurrences'),
})

// 输出 Schema
const outputSchema = z.object({
  success: z.boolean(),
  content: z.string().optional(),
  filePath: z.string(),
  message: z.string().optional(),
})
```

### 1.2 常量定义

```typescript
// 最大编辑文件大小：1 GiB
const MAX_EDIT_FILE_SIZE = 1024 * 1024 * 1024

// 文件未变化提示
const FILE_UNEXPECTEDLY_MODIFIED_ERROR = 
  'File has been modified since read, either by the user or by a linter.'

// 文件未找到提示
const FILE_NOT_FOUND_CWD_NOTE = 
  'Note: The file does not exist in the current working directory'
```

---

## 2. 输入验证流程（14 步）

### 2.1 验证步骤详解

```typescript
async function validateInput(
  input: FileEditInput,
  toolUseContext: ToolUseContext,
): Promise<ValidationResult> {
  const { file_path, old_string, new_string, replace_all = false } = input
  const fullFilePath = expandPath(file_path)
  
  // ========== 第 1 步：团队内存机密检查 ==========
  const secretError = checkTeamMemSecrets(fullFilePath, new_string)
  if (secretError) {
    return { result: false, message: secretError, errorCode: 0 }
  }
  
  // ========== 第 2 步：空字符串检查 ==========
  if (old_string === new_string) {
    return {
      result: false,
      message: 'No changes to make: old_string and new_string are exactly the same.',
      errorCode: 1,
    }
  }
  
  // ========== 第 3 步：权限否认规则检查 ==========
  const denyRule = matchingRuleForInput(
    fullFilePath,
    appState.toolPermissionContext,
    'edit',
    'deny',
  )
  if (denyRule !== null) {
    return {
      result: false,
      message: 'File is in a directory that is denied by your permission settings.',
      errorCode: 2,
    }
  }
  
  // ========== 第 4 步：UNC 路径检查（安全） ==========
  if (fullFilePath.startsWith('\\\\') || fullFilePath.startsWith('//')) {
    return { result: true } // 让权限检查处理
  }
  
  // ========== 第 5 步：文件大小检查（防 OOM） ==========
  try {
    const { size } = await fs.stat(fullFilePath)
    if (size > MAX_EDIT_FILE_SIZE) {
      return {
        result: false,
        message: `File is too large to edit (${formatFileSize(size)}). ` +
                 `Maximum editable file size is ${formatFileSize(MAX_EDIT_FILE_SIZE)}.`,
        errorCode: 10,
      }
    }
  } catch (e) {
    if (!isENOENT(e)) throw e // 文件不存在是允许的
  }
  
  // ========== 第 6 步：文件存在性检查 ==========
  let fileContent: string | null
  try {
    const fileBuffer = await fs.readFileBytes(fullFilePath)
    const encoding: BufferEncoding =
      fileBuffer.length >= 2 &&
      fileBuffer[0] === 0xff &&
      fileBuffer[1] === 0xfe
        ? 'utf16le'
        : 'utf8'
    fileContent = fileBuffer.toString(encoding).replaceAll('\r\n', '\n')
  } catch (e) {
    if (isENOENT(e)) {
      fileContent = null
    } else {
      throw e
    }
  }
  
  // ========== 第 7 步：不存在的文件处理 ==========
  if (fileContent === null) {
    // 空 old_string 表示新建文件 — 有效
    if (old_string === '') {
      return { result: true }
    }
    // 尝试找相似文件
    const similarFilename = findSimilarFile(fullFilePath)
    const cwdSuggestion = await suggestPathUnderCwd(fullFilePath)
    let message = `File does not exist.`
    
    if (cwdSuggestion) {
      message += ` Did you mean ${cwdSuggestion}?`
    } else if (similarFilename) {
      message += ` Did you mean ${similarFilename}?`
    }
    
    return {
      result: false,
      message,
      errorCode: 4,
    }
  }
  
  // ========== 第 8 步：空 old_string 检查 ==========
  if (old_string === '') {
    // 仅在文件内容为空时允许
    if (fileContent.trim() !== '') {
      return {
        result: false,
        message: 'Cannot create new file - file already exists.',
        errorCode: 3,
      }
    }
    return { result: true }
  }
  
  // ========== 第 9 步：Notebook 文件检查 ==========
  if (fullFilePath.endsWith('.ipynb')) {
    return {
      result: false,
      message: `File is a Jupyter Notebook. Use the NotebookEditTool to edit this file.`,
      errorCode: 5,
    }
  }
  
  // ========== 第 10 步：读取状态检查 ==========
  const readTimestamp = toolUseContext.readFileState.get(fullFilePath)
  if (!readTimestamp || readTimestamp.isPartialView) {
    return {
      result: false,
      message: 'File has not been read yet. Read it first before writing to it.',
      errorCode: 6,
    }
  }
  
  // ========== 第 11 步：文件修改时间检查 ==========
  if (readTimestamp) {
    const lastWriteTime = getFileModificationTime(fullFilePath)
    if (lastWriteTime > readTimestamp.timestamp) {
      // 完整读取时比较内容作为后备
      const isFullRead =
        readTimestamp.offset === undefined &&
        readTimestamp.limit === undefined
      if (isFullRead && fileContent === readTimestamp.content) {
        // 内容未变化，安全
      } else {
        return {
          result: false,
          message: 'File has been modified since read, either by the user or by a linter. ' +
                   'Read it again before attempting to write it.',
          errorCode: 7,
        }
      }
    }
  }
  
  // ========== 第 12 步：字符串匹配检查 ==========
  const actualOldString = findActualString(fileContent, old_string)
  if (!actualOldString) {
    return {
      result: false,
      message: `String to replace not found in file.`,
      errorCode: 8,
    }
  }
  
  // ========== 第 13 步：多匹配检查 ==========
  const matches = fileContent.split(actualOldString).length - 1
  if (matches > 1 && !replace_all) {
    return {
      result: false,
      message: `Found ${matches} matches of the string to replace, ` +
               `but replace_all is false. To replace all occurrences, ` +
               `set replace_all to true. To replace only one occurrence, ` +
               `please provide more context to uniquely identify the instance.`,
      errorCode: 9,
    }
  }
  
  // ========== 第 14 步：设置文件验证 ==========
  const settingsValidationResult = validateInputForSettingsFileEdit(
    fullFilePath,
    fileContent,
    () => replace_all
      ? fileContent.replaceAll(actualOldString, new_string)
      : fileContent.replace(actualOldString, new_string),
  )
  
  if (settingsValidationResult !== null) {
    return settingsValidationResult
  }
  
  return { result: true, meta: { actualOldString } }
}
```

### 2.3 错误代码定义

| 错误码 | 含义 |
|--------|------|
| 0 | 检测到机密信息 |
| 1 | old_string 和 new_string 相同 |
| 2 | 权限拒绝 |
| 3 | 无法创建文件（已存在） |
| 4 | 文件不存在 |
| 5 | Notebook 文件需用专用工具 |
| 6 | 文件未读取 |
| 7 | 文件已被修改 |
| 8 | 字符串未找到 |
| 9 | 多个匹配但 replace_all=false |
| 10 | 文件过大 |

---

## 3. 关键安全特性

### 3.1 团队内存机密保护

```typescript
// checkTeamMemSecrets - 检查是否引入机密
function checkTeamMemSecrets(
  filePath: string,
  newContent: string,
): string | null {
  // 仅检查团队内存文件
  if (!isTeamMemFile(filePath)) {
    return null
  }
  
  // 检测常见机密模式
  const secretPatterns = [
    /api[_-]?key\s*[:=]\s*['"][a-zA-Z0-9]{20,}['"]/i,
    /password\s*[:=]\s*['"].+['"]/i,
    /secret\s*[:=]\s*['"].+['"]/i,
    /token\s*[:=]\s*['"][a-zA-Z0-9]{20,}['"]/i,
    /-----BEGIN (RSA |EC )?PRIVATE KEY-----/,
  ]
  
  for (const pattern of secretPatterns) {
    if (pattern.test(newContent)) {
      return 'Cannot write secrets to team memory. ' +
             'Please use environment variables or a secrets manager instead.'
    }
  }
  
  return null
}
```

### 3.2 文件修改时间（mtime）检查

```typescript
// getFileModificationTime - 获取文件最后修改时间
function getFileModificationTime(filePath: string): number {
  try {
    const stat = fs.statSync(filePath)
    return stat.mtimeMs
  } catch {
    return 0 // 文件不存在
  }
}

// 比较逻辑
if (lastWriteTime > readTimestamp.timestamp) {
  // 时间戳被修改，但可能是误报（云同步、杀毒软件等）
  // 对于完整读取，比较内容作为后备
  const isFullRead = !readTimestamp.offset && !readTimestamp.limit
  if (isFullRead && fileContent === readTimestamp.content) {
    // 内容未变，安全继续
  } else {
    throw new Error(FILE_UNEXPECTEDLY_MODIFIED_ERROR)
  }
}
```

### 3.3 引号风格保留

```typescript
// preserveQuoteStyle - 保留文件中的引号风格
function preserveQuoteStyle(
  oldString: string,
  actualOldString: string,
  newString: string,
): string {
  // 检测文件中的引号类型
  const hasCurlyQuotes = actualOldString.includes('"') || 
                         actualOldString.includes('"') ||
                         actualOldString.includes(''') ||
                         actualOldString.includes(''')
  
  // 如果文件使用弯引号，转换新字符串
  if (hasCurlyQuotes) {
    return convertStraightQuotesToCurly(newString)
  }
  
  return newString
}
```

### 3.4 编码和行尾检测

```typescript
// 读取文件时检测编码
const fileBuffer = await fs.readFileBytes(fullFilePath)
const encoding: BufferEncoding =
  fileBuffer.length >= 2 &&
  fileBuffer[0] === 0xff &&
  fileBuffer[1] === 0xfe
    ? 'utf16le'  // BOM 标记的 UTF-16
    : 'utf8'     // 默认 UTF-8

// 行尾检测
function detectLineEndings(content: string): LineEndingType {
  if (content.includes('\r\n')) return 'crlf'  // Windows
  if (content.includes('\r')) return 'cr'      // Classic Mac
  return 'lf'                                   // Unix
}

// 写入时保留原有风格
writeTextContent(
  absoluteFilePath,
  updatedFile,
  encoding,      // 保留编码
  endings,       // 保留行尾
)
```

---

## 4. 原子性写保护

### 4.1 临界区保护

```typescript
// 确保父目录存在（原子操作外）
await fs.mkdir(dirname(absoluteFilePath))

// 文件历史备份（在临界区外，幂等操作）
if (fileHistoryEnabled()) {
  await fileHistoryTrackEdit(
    updateFileHistoryState,
    absoluteFilePath,
    parentMessage.uuid,
  )
}

// ========== 临界区开始 ==========
// 这些 await 必须在写入部分之外，以保持原子性
// 在陈旧检查和 writeTextContent 之间 yield 会让并发编辑交错

// 加载当前状态并确认自上次读取后无变化
const {
  content: originalFileContents,
  fileExists,
  encoding,
  lineEndings: endings,
} = readFileForEdit(absoluteFilePath)

// 再次检查 mtime（双重检查）
if (fileExists) {
  const lastWriteTime = getFileModificationTime(absoluteFilePath)
  const lastRead = readFileState.get(absoluteFilePath)
  if (!lastRead || lastWriteTime > lastRead.timestamp) {
    const isFullRead = lastRead?.offset === undefined && lastRead?.limit === undefined
    const contentUnchanged = isFullRead && originalFileContents === lastRead.content
    if (!contentUnchanged) {
      throw new Error(FILE_UNEXPECTEDLY_MODIFIED_ERROR)
    }
  }
}

// 生成补丁
const { patch, updatedFile } = getPatchForEdit({
  filePath: absoluteFilePath,
  fileContents: originalFileContents,
  oldString: actualOldString,
  newString: actualNewString,
  replaceAll: replace_all,
})

// 写入磁盘（原子操作）
writeTextContent(absoluteFilePath, updatedFile, encoding, endings)
// ========== 临界区结束 ==========
```

### 4.2 LSP 通知

```typescript
// 通知 LSP 服务器文件修改
const lspManager = getLspServerManager()
if (lspManager) {
  // 清除先前发送的诊断
  clearDeliveredDiagnosticsForFile(`file://${absoluteFilePath}`)
  
  // didChange: 内容已修改
  lspManager.notifyDidChangeTextDocument(absoluteFilePath, updatedFile)
  
  // didSave: 文件已保存
  lspManager.notifyDidSaveTextDocument(absoluteFilePath)
}
```

### 4.3 Git 差异计算

```typescript
// 计算编辑前后的 Git 差异
const diff = await fetchSingleFileGitDiff(
  absoluteFilePath,
  originalFileContents,
  updatedFile,
)

// 记录行数变化
const linesAdded = countLinesChanged(originalFileContents, updatedFile, 'added')
const linesRemoved = countLinesChanged(originalFileContents, updatedFile, 'removed')
```

---

## 5. 权限系统集成

### 6.1 权限检查流程

```typescript
// checkPermissions - 检查编辑权限
async function checkPermissions(
  input: FileEditInput,
  context: ToolUseContext,
): Promise<PermissionDecision> {
  const appState = context.getAppState()
  return checkWritePermissionForTool(
    FileEditTool,
    input,
    appState.toolPermissionContext,
  )
}

// checkWritePermissionForTool - 通用写入权限检查
export async function checkWritePermissionForTool(
  tool: Tool,
  input: unknown,
  context: ToolPermissionContext,
): Promise<PermissionDecision> {
  // 1. 检查 bypass 模式
  if (context.mode === 'bypass') {
    return { behavior: 'allow', source: 'mode' }
  }
  
  // 2. 检查显式允许规则
  const allowRule = findMatchingRule(context.alwaysAllowRules, tool, input)
  if (allowRule) {
    return { behavior: 'allow', source: allowRule.source }
  }
  
  // 3. 检查显式拒绝规则
  const denyRule = findMatchingRule(context.neverAllowRules, tool, input)
  if (denyRule) {
    return { behavior: 'deny', source: denyRule.source }
  }
  
  // 4. 默认需要询问
  return { behavior: 'ask' }
}
```

### 6.2 规则匹配优先级

```typescript
// 规则来源优先级（从高到低）
const PERMISSION_RULE_SOURCES = [
  // 用户设置 (~/.config/yode/config.toml)
  { name: 'user', priority: 1 },
  // 项目设置 (.yode/config.toml)
  { name: 'project', priority: 2 },
  // 本地设置
  { name: 'local', priority: 3 },
  // Flag 设置
  { name: 'flag', priority: 4 },
  // Policy 设置
  { name: 'policy', priority: 5 },
  // CLI 参数
  { name: 'cliArg', priority: 6 },
  // 命令
  { name: 'command', priority: 7 },
  // 会话
  { name: 'session', priority: 8 },
]

// 找到第一个匹配的规则
function findFirstMatchingRule(
  rules: Record<PermissionRuleSource, string[]>,
  toolName: string,
  input: unknown,
): PermissionRule | null {
  for (const source of PERMISSION_RULE_SOURCES) {
    const sourceRules = rules[source.name] || []
    for (const ruleString of sourceRules) {
      const rule = parseRule(ruleString)
      if (matchesRule(rule, toolName, input)) {
        return { ...rule, source: source.name }
      }
    }
  }
  return null
}
```

---

## 6. Yode FileEditTool 优化建议

### 6.1 第一阶段：14 步验证流程

```rust
// crates/yode-tools/src/builtin/edit_file/validation.rs

use std::path::Path;

/// 验证错误类型
#[derive(Debug, Clone)]
pub enum EditValidationError {
    SecretDetected(String),
    NoChanges,
    PermissionDenied,
    UncPath,
    FileTooLarge { size: usize, max: usize },
    FileNotFound { suggestion: Option<String> },
    FileExists,
    NotebookFile,
    FileNotRead,
    FileModified,
    StringNotFound,
    MultipleMatches { count: usize },
    SettingsValidationFailed(String),
}

/// 验证器
pub struct EditValidator {
    max_file_size: usize,
}

impl EditValidator {
    pub fn new(max_file_size: usize) -> Self {
        Self { max_file_size }
    }
    
    /// 执行完整验证流程
    pub async fn validate(
        &self,
        input: &EditInput,
        context: &EditContext,
    ) -> Result<(), EditValidationError> {
        let full_path = expand_path(&input.file_path);
        
        // 1. 机密检查
        check_team_mem_secrets(&full_path, &input.new_string)?;
        
        // 2. 空字符串检查
        if input.old_string == input.new_string {
            return Err(EditValidationError::NoChanges);
        }
        
        // 3. 权限检查
        check_permission(&full_path, "edit")?;
        
        // 4. UNC 路径检查
        if is_unc_path(&full_path) {
            return Ok(()); // 让权限检查处理
        }
        
        // 5. 文件大小检查
        if let Ok(metadata) = tokio::fs::metadata(&full_path).await {
            if metadata.len() > self.max_file_size as u64 {
                return Err(EditValidationError::FileTooLarge {
                    size: metadata.len() as usize,
                    max: self.max_file_size,
                });
            }
        }
        
        // 6-14. 其余验证...
        self.validate_file_content(&full_path, input, context).await?;
        
        Ok(())
    }
}
```

### 6.2 第二阶段：mtime 检查

```rust
// crates/yode-tools/src/builtin/edit_file/mtime.rs

use std::time::SystemTime;

/// 文件读取状态
#[derive(Debug, Clone)]
pub struct FileReadState {
    pub timestamp: SystemTime,
    pub content_hash: u64,  // 内容哈希
    pub is_partial_view: bool,
    pub offset: Option<usize>,
    pub limit: Option<usize>,
}

/// 检查文件是否被修改
pub fn is_file_modified(
    path: &str,
    read_state: &FileReadState,
) -> Result<bool, std::io::Error> {
    let metadata = std::fs::metadata(path)?;
    let mtime = metadata.modified()?;
    
    // 时间戳被修改
    if mtime > read_state.timestamp {
        // 对于完整读取，使用内容哈希作为后备
        if !read_state.is_partial_view {
            let current_hash = compute_content_hash(path)?;
            if current_hash == read_state.content_hash {
                // 内容未变，安全
                return Ok(false);
            }
        }
        return Ok(true);
    }
    
    Ok(false)
}

/// 计算内容哈希（用于快速比较）
fn compute_content_hash(path: &str) -> Result<u64, std::io::Error> {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    
    let content = std::fs::read_to_string(path)?;
    let mut hasher = DefaultHasher::new();
    content.hash(&mut hasher);
    Ok(hasher.finish())
}
```

### 6.3 第三阶段：编码和行尾保留

```rust
// crates/yode-tools/src/builtin/edit_file/encoding.rs

/// 文件编码
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FileEncoding {
    Utf8,
    Utf16Le,
    Utf16Be,
    Latin1,
}

/// 行尾类型
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LineEnding {
    Lf,      // Unix
    CrLf,    // Windows
    Cr,      // Classic Mac
}

/// 检测文件编码
pub fn detect_encoding(content: &[u8]) -> FileEncoding {
    // 检查 BOM
    if content.len() >= 2 {
        if content[0] == 0xff && content[1] == 0xfe {
            return FileEncoding::Utf16Le;
        }
        if content[0] == 0xfe && content[1] == 0xff {
            return FileEncoding::Utf16Be;
        }
    }
    
    // 检查 UTF-8 BOM
    if content.len() >= 3 && content[0] == 0xef && content[1] == 0xbb && content[2] == 0xbf {
        return FileEncoding::Utf8;
    }
    
    // 默认 UTF-8
    FileEncoding::Utf8
}

/// 检测行尾
pub fn detect_line_ending(content: &str) -> LineEnding {
    if content.contains("\r\n") {
        LineEnding::CrLf
    } else if content.contains('\r') {
        LineEnding::Cr
    } else {
        LineEnding::Lf
    }
}

/// 写入文件时保留编码和行尾
pub fn write_text_content(
    path: &str,
    content: &str,
    encoding: FileEncoding,
    line_ending: LineEnding,
) -> Result<(), std::io::Error> {
    // 转换行尾
    let normalized = match line_ending {
        LineEnding::Lf => content.replace("\r\n", "\n").replace('\r', "\n"),
        LineEnding::CrLf => content.replace('\n', "\r\n").replace('\r', "\r\n"),
        LineEnding::Cr => content.replace("\r\n", "\r").replace('\n', "\r"),
    };
    
    // 转换编码
    let bytes = match encoding {
        FileEncoding::Utf8 => normalized.into_bytes(),
        FileEncoding::Utf16Le => {
            use encoding_rs::UTF_16LE;
            UTF_16LE.encode(&normalized).0.into_owned()
        }
        FileEncoding::Utf16Be => {
            use encoding_rs::UTF_16BE;
            UTF_16BE.encode(&normalized).0.into_owned()
        }
        FileEncoding::Latin1 => {
            use encoding_rs::WINDOWS_1252;
            WINDOWS_1252.encode(&normalized).0.into_owned()
        }
    };
    
    std::fs::write(path, bytes)
}
```

### 6.4 第四阶段：字符串匹配（带引号规范化）

```rust
// crates/yode-tools/src/builtin/edit_file/string_match.rs

/// 查找实际字符串（处理引号规范化）
pub fn find_actual_string(file_content: &str, target: &str) -> Option<String> {
    // 直接匹配
    if file_content.contains(target) {
        return Some(target.to_string());
    }
    
    // 规范化引号后匹配
    let normalized_target = normalize_quotes(target);
    if file_content.contains(&normalized_target) {
        return Some(normalized_target);
    }
    
    // 尝试弯引号
    let curly_target = convert_to_curly_quotes(target);
    if file_content.contains(&curly_target) {
        return Some(curly_target);
    }
    
    None
}

/// 规范化引号
fn normalize_quotes(s: &str) -> String {
    s.replace('"', "\"")
     .replace('"', "\"")
     .replace(''', "'")
     .replace(''', "'")
}

/// 转换为弯引号
fn convert_to_curly_quotes(s: &str) -> String {
    // 简单转换：直引号 -> 弯引号
    s.replace('"', """)
     .replace('"', """)
     .replace('\'', "'")
     .replace('\'', "'")
}

/// 计算匹配数
pub fn count_matches(file_content: &str, target: &str) -> usize {
    file_content.split(target).count() - 1
}
```

---

## 7. 错误消息模板

```rust
// crates/yode-tools/src/builtin/edit_file/messages.rs

/// 错误消息生成
pub fn format_error_message(error: &EditValidationError) -> String {
    match error {
        EditValidationError::SecretDetected(msg) => msg.clone(),
        EditValidationError::NoChanges => 
            "No changes to make: old_string and new_string are exactly the same.".to_string(),
        EditValidationError::PermissionDenied => 
            "File is in a directory that is denied by your permission settings.".to_string(),
        EditValidationError::FileTooLarge { size, max } => 
            format!("File is too large to edit ({}). Maximum file size is {}.", 
                    format_size(*size), format_size(*max)),
        EditValidationError::FileNotFound { suggestion } => {
            let mut msg = "File does not exist.".to_string();
            if let Some(s) = suggestion {
                msg.push_str(&format!(" Did you mean {}?", s));
            }
            msg
        }
        EditValidationError::FileExists => 
            "Cannot create new file - file already exists.".to_string(),
        EditValidationError::NotebookFile => 
            "File is a Jupyter Notebook. Use the NotebookEditTool to edit this file.".to_string(),
        EditValidationError::FileNotRead => 
            "File has not been read yet. Read it first before writing to it.".to_string(),
        EditValidationError::FileModified => 
            "File has been modified since read, either by the user or by a linter. \
             Read it again before attempting to write it.".to_string(),
        EditValidationError::StringNotFound => 
            "String to replace not found in file.".to_string(),
        EditValidationError::MultipleMatches { count } => 
            format!("Found {} matches of the string to replace, but replace_all is false. \
                     To replace all occurrences, set replace_all to true. \
                     To replace only one occurrence, provide more context to uniquely \
                     identify the instance.", count),
        EditValidationError::SettingsValidationFailed(msg) => msg.clone(),
    }
}
```

---

## 8. 配置文件示例

```toml
# ~/.config/yode/config.toml

[tools.file_edit]
# 最大文件大小
max_file_size_bytes = 1073741824  # 1 GiB

# 验证设置
[tools.file_edit.validation]
check_mtime = true
check_read_state = true
check_secrets = true

# 编码处理
[tools.file_edit.encoding]
detect_automatically = true
preserve_line_endings = true

# 字符串匹配
[tools.file_edit.matching]
normalize_quotes = true
fuzzy_match = false

# 原子性
[tools.file_edit.atomicity]
use_critical_section = true
backup_before_edit = true
```

---

## 9. 总结

Claude Code FileEditTool 的核心特点：

1. **14 步验证流程** - 全面的安全检查
2. **机密保护** - 阻止写入团队内存机密
3. **mtime 检查** - 检测文件外部修改
4. **内容哈希后备** - 避免时间戳误报
5. **编码保留** - UTF-8/UTF-16/Latin1 自动检测
6. **行尾保留** - LF/CRLF/CR 自动保留
7. **引号规范化** - 直引号/弯引号处理
8. **多匹配检测** - 防止意外批量替换
9. **Notebook 专用** - .ipynb 强制使用专用工具
10. **LSP 集成** - 编辑后通知 LSP 服务器
11. **Git 差异** - 自动计算编辑差异
12. **原子性保护** - 临界区防止并发编辑

Yode 优化优先级：
1. 14 步验证流程框架
2. mtime + 内容哈希检查
3. 编码和行尾检测保留
4. 引号规范化匹配
5. 机密检测保护
6. LSP 通知集成
