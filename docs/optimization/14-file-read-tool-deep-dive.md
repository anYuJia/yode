# FileReadTool 深度分析与优化

## 1. FileReadTool 架构概述

Claude Code 的 FileReadTool (`src/tools/FileReadTool/FileReadTool.ts`) 是一个支持多种文件类型的读取工具，包括文本文件、图片、PDF、Jupyter Notebook 等。

### 1.1 工具输入输出 Schema

```typescript
// 输入 Schema
const inputSchema = z.strictObject({
  file_path: z.string().describe('The absolute path to the file to read'),
  offset: semanticNumber(z.number().int().nonnegative().optional()).describe(
    'The line number to start reading from. Only provide if the file is too large',
  ),
  limit: semanticNumber(z.number().int().positive().optional()).describe(
    'The number of lines to read. Only provide if the file is too large',
  ),
  pages: z.string().optional().describe(
    `Page range for PDF files (e.g., "1-5", "3", "10-20"). Maximum ${PDF_MAX_PAGES_PER_READ} pages.`,
  ),
})

// 输出 Schema - 多态类型
const outputSchema = z.discriminatedUnion('type', [
  // 文本文件
  z.object({
    type: z.literal('text'),
    file: z.object({
      filePath: z.string(),
      content: z.string(),
      numLines: z.number(),
      startLine: z.number(),
      totalLines: z.number(),
    }),
  }),
  // 图片文件
  z.object({
    type: z.literal('image'),
    file: z.object({
      base64: z.string(),
      type: z.enum(['image/jpeg', 'image/png', 'image/gif', 'image/webp']),
      originalSize: z.number(),
      dimensions: z.object({
        originalWidth: z.number().optional(),
        originalHeight: z.number().optional(),
        displayWidth: z.number().optional(),
        displayHeight: z.number().optional(),
      }).optional(),
    }),
  }),
  // Jupyter Notebook
  z.object({
    type: z.literal('notebook'),
    file: z.object({
      filePath: z.string(),
      cells: z.array(z.any()),
    }),
  }),
  // PDF 文件
  z.object({
    type: z.literal('pdf'),
    file: z.object({
      filePath: z.string(),
      base64: z.string(),
      originalSize: z.number(),
    }),
  }),
  // PDF 分页提取
  z.object({
    type: z.literal('parts'),
    file: z.object({
      filePath: z.string(),
      originalSize: z.number(),
      count: z.number(),
      outputDir: z.string(),
    }),
  }),
  // 文件未变化（优化）
  z.object({
    type: z.literal('file_unchanged'),
    file: z.object({
      filePath: z.string(),
    }),
  }),
])
```

### 1.2 支持的文件类型

```typescript
// 图片扩展名
const IMAGE_EXTENSIONS = new Set(['png', 'jpg', 'jpeg', 'gif', 'webp'])

// PDF 检测
function isPDFExtension(ext: string): boolean {
  return ext.toLowerCase() === '.pdf'
}

// 二进制文件检测（排除 PDF、图片、SVG）
function hasBinaryExtension(filePath: string): boolean {
  const ext = path.extname(filePath).toLowerCase()
  if (isPDFExtension(ext) || IMAGE_EXTENSIONS.has(ext.slice(1))) {
    return false // 这些文件由本工具原生处理
  }
  // 检查是否为已知二进制扩展名
  return BINARY_EXTENSIONS.has(ext)
}
```

### 1.3 阻止的设备路径

```typescript
// 会导致进程挂起或无限输出的设备文件
const BLOCKED_DEVICE_PATHS = new Set([
  // 无限输出
  '/dev/zero',
  '/dev/random',
  '/dev/urandom',
  '/dev/full',
  // 阻塞等待输入
  '/dev/stdin',
  '/dev/tty',
  '/dev/console',
  // 无意义读取
  '/dev/stdout',
  '/dev/stderr',
  '/dev/fd/0',
  '/dev/fd/1',
  '/dev/fd/2',
])

function isBlockedDevicePath(filePath: string): boolean {
  if (BLOCKED_DEVICE_PATHS.has(filePath)) return true
  
  // Linux /proc 别名检查
  if (
    filePath.startsWith('/proc/') &&
    (filePath.endsWith('/fd/0') ||
      filePath.endsWith('/fd/1') ||
      filePath.endsWith('/fd/2'))
  ) {
    return true
  }
  
  return false
}
```

---

## 2. 输入验证流程

### 2.1 验证步骤（无 I/O 优先）

```typescript
async function validateInput(
  { file_path, pages }: Input,
  toolUseContext: ToolUseContext,
): Promise<ValidationResult> {
  // 1. Pages 参数验证（纯字符串解析，无 I/O）
  if (pages !== undefined) {
    const parsed = parsePDFPageRange(pages)
    if (!parsed) {
      return {
        result: false,
        message: `Invalid pages parameter: "${pages}"`,
        errorCode: 7,
      }
    }
    const rangeSize = parsed.lastPage === Infinity
      ? PDF_MAX_PAGES_PER_READ + 1
      : parsed.lastPage - parsed.firstPage + 1
    if (rangeSize > PDF_MAX_PAGES_PER_READ) {
      return {
        result: false,
        message: `Page range "${pages}" exceeds maximum of ${PDF_MAX_PAGES_PER_READ} pages`,
        errorCode: 8,
      }
    }
  }
  
  // 2. 路径扩展 + 否认规则检查（无 I/O）
  const fullFilePath = expandPath(file_path)
  const denyRule = matchingRuleForInput(
    fullFilePath,
    appState.toolPermissionContext,
    'read',
    'deny',
  )
  if (denyRule !== null) {
    return {
      result: false,
      message: 'File is in a directory that is denied by your permission settings.',
      errorCode: 1,
    }
  }
  
  // 3. UNC 路径检查（无 I/O，安全预防）
  const isUncPath = fullFilePath.startsWith('\\\\') || fullFilePath.startsWith('//')
  if (isUncPath) {
    return { result: true } // 让权限检查处理
  }
  
  // 4. 二进制扩展名检查（仅字符串检查，无 I/O）
  const ext = path.extname(fullFilePath).toLowerCase()
  if (
    hasBinaryExtension(fullFilePath) &&
    !isPDFExtension(ext) &&
    !IMAGE_EXTENSIONS.has(ext.slice(1))
  ) {
    return {
      result: false,
      message: `This tool cannot read binary files. The file appears to be a binary ${ext} file.`,
      errorCode: 4,
    }
  }
  
  // 5. 阻止的设备路径检查（无 I/O）
  if (isBlockedDevicePath(fullFilePath)) {
    return {
      result: false,
      message: `Cannot read '${file_path}': this device file would block or produce infinite output.`,
      errorCode: 9,
    }
  }
  
  return { result: true }
}
```

### 2.2 错误代码定义

| 错误码 | 含义 |
|--------|------|
| 1 | 权限拒绝 |
| 4 | 二进制文件不支持 |
| 7 | Pages 参数格式无效 |
| 8 | Pages 范围超出限制 |
| 9 | 阻止的设备路径 |
| 10 | 文件过大 |

---

## 3. PDF 处理

### 3.1 PDF 常量定义

```typescript
// 每次读取的最大页数
const PDF_MAX_PAGES_PER_READ = 20

// 提及内联的 token 阈值（用于 PDF @mention）
const PDF_AT_MENTION_INLINE_THRESHOLD = 10

// PDF 提取大小阈值（超过则提取为单独文件）
const PDF_EXTRACT_SIZE_THRESHOLD = 5 * 1024 * 1024 // 5MB
```

### 3.2 PDF 页码范围解析

```typescript
// parsePDFPageRange - 解析 PDF 页码范围
function parsePDFPageRange(pages: string): { firstPage: number, lastPage: number } | null {
  // 单页： "3" -> { firstPage: 3, lastPage: 3 }
  // 范围： "1-5" -> { firstPage: 1, lastPage: 5 }
  // 开放范围： "10-" -> { firstPage: 10, lastPage: Infinity }
  
  const match = pages.match(/^(\d+)(?:-(\d+)?)?$/)
  if (!match) return null
  
  const firstPage = parseInt(match[1], 10)
  const lastPage = match[2] ? parseInt(match[2], 10) : Infinity
  
  return { firstPage, lastPage }
}
```

### 3.3 PDF 读取流程

```typescript
// 读取 PDF 文件
async function readPDFFile(
  filePath: string,
  pages?: string,
): Promise<Output> {
  // 1. 获取 PDF 总页数
  const pageCount = await getPDFPageCount(filePath)
  
  // 2. 解析页码范围
  const pageRange = parsePDFPageRange(pages || '1')
  
  // 3. 提取指定页面
  const pdfContent = await extractPDFPages(
    filePath,
    pageRange.firstPage,
    pageRange.lastPage,
  )
  
  // 4. 检查是否需要分离输出
  if (pdfContent.length > PDF_EXTRACT_SIZE_THRESHOLD) {
    // 写入临时目录，返回 parts 类型
    return {
      type: 'parts',
      file: {
        filePath,
        originalSize: pdfContent.length,
        count: pageRange.lastPage - pageRange.firstPage + 1,
        outputDir: tempDir,
      },
    }
  }
  
  // 5. 返回 base64 编码
  return {
    type: 'pdf',
    file: {
      filePath,
      base64: pdfContent.toString('base64'),
      originalSize: pdfContent.length,
    },
  }
}
```

---

## 4. 图片处理

### 4.1 图片检测与调整

```typescript
// 检测图片格式
function detectImageFormatFromBuffer(buffer: Buffer): string | null {
  // 检查魔数
  if (buffer[0] === 0x89 && buffer[1] === 0x50 && buffer[2] === 0x4e) {
    return 'image/png'
  }
  if (buffer[0] === 0xff && buffer[1] === 0xd8) {
    return 'image/jpeg'
  }
  if (buffer[0] === 0x47 && buffer[1] === 0x49 && buffer[2] === 0x46) {
    return 'image/gif'
  }
  if (buffer[0] === 0x52 && buffer[1] === 0x49 && buffer[2] === 0x46) {
    return 'image/webp'
  }
  return null
}

// 调整图片大小
async function maybeResizeAndDownsampleImageBuffer(
  buffer: Buffer,
  maxWidth: number = 2048,
): Promise<{ buffer: Buffer, dimensions: ImageDimensions }> {
  // 加载图片
  const image = await loadImage(buffer)
  
  const originalWidth = image.width
  const originalHeight = image.height
  
  // 计算缩放比例
  const scale = Math.min(1, maxWidth / originalWidth)
  
  const displayWidth = Math.floor(originalWidth * scale)
  const displayHeight = Math.floor(originalHeight * scale)
  
  // 调整大小
  const resized = await resizeImage(image, displayWidth, displayHeight)
  
  // 压缩并返回
  const compressedBuffer = await compressImage(resized)
  
  return {
    buffer: compressedBuffer,
    dimensions: {
      originalWidth,
      originalHeight,
      displayWidth,
      displayHeight,
    },
  }
}
```

### 4.2 图片 Token 压缩

```typescript
// 根据 token 限制压缩图片
async function compressImageBufferWithTokenLimit(
  imageBuffer: Buffer,
  maxTokens: number,
): Promise<Buffer> {
  // 估算当前图片的 token 数
  let tokens = estimateImageTokens(imageBuffer)
  
  let quality = 0.95
  let buffer = imageBuffer
  
  while (tokens > maxTokens && quality > 0.1) {
    quality -= 0.05
    buffer = await compressImage(buffer, quality)
    tokens = estimateImageTokens(buffer)
  }
  
  return buffer
}

// 估算图片 token 数
function estimateImageTokens(buffer: Buffer): number {
  // 基于图片大小的粗略估算
  // 通常 1KB 图片 ≈ 100-200 tokens
  return Math.floor(buffer.length / 1024 * 150)
}
```

---

## 5. Jupyter Notebook 处理

### 5.1 Notebook 单元格映射

```typescript
// mapNotebookCellsToToolResult - 将 Notebook 单元格映射到工具结果
function mapNotebookCellsToToolResult(
  notebook: NotebookDocument,
  filePath: string,
): Output {
  const cells = notebook.cells.map(cell => ({
    cell_type: cell.cell_type,
    source: cell.source,
    outputs: cell.outputs,
    metadata: cell.metadata,
  }))
  
  return {
    type: 'notebook',
    file: {
      filePath,
      cells,
    },
  }
}

// 读取 Notebook 文件
async function readNotebook(filePath: string): Promise<NotebookDocument> {
  const content = await fs.readFile(filePath, 'utf-8')
  const notebook = JSON.parse(content) as NotebookDocument
  
  // 验证 Notebook 结构
  if (!notebook.cells || !Array.isArray(notebook.cells)) {
    throw new Error('Invalid notebook format: missing cells array')
  }
  
  return notebook
}
```

---

## 6. 大文件处理

### 6.1 Token 限制检查

```typescript
// 最大 token 限制
const MAX_FILE_READ_TOKENS = 100_000

// Token 超限错误
export class MaxFileReadTokenExceededError extends Error {
  constructor(
    public tokenCount: number,
    public maxTokens: number,
  ) {
    super(
      `File content (${tokenCount} tokens) exceeds maximum allowed tokens (${maxTokens}). ` +
      'Use offset and limit parameters to read specific portions of the file, ' +
      'or search for specific content instead of reading the whole file.',
    )
    this.name = 'MaxFileReadTokenExceededError'
  }
}

// Token 计数
async function countTokensWithAPI(content: string): Promise<number> {
  // 使用 API 进行精确 token 计数
  // 或使用粗略估算：每 4 字符 ≈ 1 token
  return Math.ceil(content.length / 4)
}

// 粗略估算
function roughTokenCountEstimationForFileType(content: string, ext: string): number {
  // 不同文件类型的 token 密度不同
  const factors: Record<string, number> = {
    '.ts': 3.5,
    '.tsx': 3.5,
    '.py': 4,
    '.md': 2.5,
    '.json': 5,
  }
  
  const factor = factors[ext] || 4
  return Math.ceil(content.length / factor)
}
```

### 6.2 文件读取限制

```typescript
// 默认限制
function getDefaultFileReadingLimits() {
  return {
    // 最大文件大小
    maxSizeBytes: 10 * 1024 * 1024, // 10MB
    
    // 默认行数限制
    maxLines: 2000,
    
    // 目标范围提示（鼓励使用 offset/limit）
    targetedRangeNudge: true,
    
    // 包含最大大小提示
    includeMaxSizeInPrompt: true,
  }
}

// 带范围的文件读取
async function readFileInRange(
  filePath: string,
  offset: number,
  limit: number,
): Promise<{ content: string, totalLines: number }> {
  const allLines = await fs.readFile(filePath, 'utf-8')
    .then(content => content.split('\n'))
  
  const totalLines = allLines.length
  const startLine = Math.max(0, offset - 1)
  const endLine = Math.min(totalLines, startLine + limit)
  
  const selectedLines = allLines.slice(startLine, endLine)
  
  return {
    content: selectedLines.join('\n'),
    totalLines,
  }
}
```

---

## 7. 会话文件检测

### 7.1 会话相关文件类型

```typescript
/**
 * 检测文件路径是否为会话相关文件
 * 仅匹配 Claude 配置目录内的文件 (~/.claude)
 */
function detectSessionFileType(filePath: string): 'session_memory' | 'session_transcript' | null {
  const configDir = getClaudeConfigHomeDir()
  
  // 仅匹配配置目录内的文件
  if (!filePath.startsWith(configDir)) {
    return null
  }
  
  // 规范化路径
  const normalizedPath = filePath.split(win32.sep).join(posix.sep)
  
  // 会话内存文件：~/.claude/session-memory/*.md
  if (
    normalizedPath.includes('/session-memory/') &&
    normalizedPath.endsWith('.md')
  ) {
    return 'session_memory'
  }
  
  // 会话 JSONL 文件：~/.claude/projects/*/*.jsonl
  if (
    normalizedPath.includes('/projects/') &&
    normalizedPath.endsWith('.jsonl')
  ) {
    return 'session_transcript'
  }
  
  return null
}
```

### 7.2 文件读取监听器

```typescript
// 文件读取监听器
type FileReadListener = (filePath: string, content: string) => void
const fileReadListeners: FileReadListener[] = []

// 注册监听器
export function registerFileReadListener(
  listener: FileReadListener,
): () => void {
  fileReadListeners.push(listener)
  return () => {
    const i = fileReadListeners.indexOf(listener)
    if (i >= 0) fileReadListeners.splice(i, 1)
  }
}

// 通知监听器
function notifyFileReadListeners(filePath: string, content: string) {
  for (const listener of fileReadListeners) {
    try {
      listener(filePath, content)
    } catch (error) {
      logError('FileReadListener error:', error)
    }
  }
}
```

---

## 8. Yode FileReadTool 优化建议

### 8.1 第一阶段：多文件类型支持

```rust
// crates/yode-tools/src/builtin/read_file.rs

use std::path::Path;

/// 支持的文件类型
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FileType {
    Text,
    Image(ImageFormat),
    Pdf,
    Notebook,
    Binary,
}

/// 图片格式
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ImageFormat {
    Png,
    Jpeg,
    Gif,
    Webp,
}

/// 检测文件类型
pub fn detect_file_type(path: &Path) -> FileType {
    let ext = path.extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_lowercase())
        .unwrap_or_default();
    
    match ext.as_str() {
        "png" => FileType::Image(ImageFormat::Png),
        "jpg" | "jpeg" => FileType::Image(ImageFormat::Jpeg),
        "gif" => FileType::Image(ImageFormat::Gif),
        "webp" => FileType::Image(ImageFormat::Webp),
        "pdf" => FileType::Pdf,
        "ipynb" => FileType::Notebook,
        _ => {
            // 检查是否为二进制文件
            if is_binary_extension(&ext) {
                FileType::Binary
            } else {
                FileType::Text
            }
        }
    }
}

/// 二进制扩展名列表
fn is_binary_extension(ext: &str) -> bool {
    const BINARY_EXTS: &[&str] = &[
        "exe", "dll", "so", "dylib",
        "bin", "dat", "db", "sqlite",
        "zip", "tar", "gz", "rar", "7z",
        "pdf", "doc", "docx", "xls", "xlsx",
    ];
    BINARY_EXTS.contains(&ext)
}
```

### 8.2 第二阶段：阻止设备路径

```rust
// crates/yode-tools/src/builtin/read_file/security.rs

use std::collections::HashSet;

/// 阻止的设备路径
fn get_blocked_device_paths() -> &'static HashSet<&'static str> {
    lazy_static! {
        static ref BLOCKED: HashSet<&'static str> = [
            "/dev/zero", "/dev/random", "/dev/urandom", "/dev/full",
            "/dev/stdin", "/dev/tty", "/dev/console",
            "/dev/stdout", "/dev/stderr",
            "/dev/fd/0", "/dev/fd/1", "/dev/fd/2",
        ].iter().copied().collect();
    }
    &BLOCKED
}

/// 检查路径是否为阻止的设备路径
pub fn is_blocked_device_path(path: &str) -> bool {
    if get_blocked_device_paths().contains(path) {
        return true;
    }
    
    // Linux /proc 别名
    if path.starts_with("/proc/") 
        && (path.ends_with("/fd/0") || path.ends_with("/fd/1") || path.ends_with("/fd/2")) 
    {
        return true;
    }
    
    false
}
```

### 8.3 第三阶段：分页读取

```rust
// crates/yode-tools/src/builtin/read_file/pagination.rs

/// PDF 最大读取页数
const PDF_MAX_PAGES_PER_READ: usize = 20;

/// 默认文件读取限制
#[derive(Debug, Clone)]
pub struct FileReadingLimits {
    /// 最大文件大小（字节）
    pub max_size_bytes: usize,
    /// 最大行数
    pub max_lines: usize,
    /// 目标范围提示
    pub targeted_range_nudge: bool,
}

impl Default for FileReadingLimits {
    fn default() -> Self {
        Self {
            max_size_bytes: 10 * 1024 * 1024, // 10MB
            max_lines: 2000,
            targeted_range_nudge: true,
        }
    }
}

/// 带范围的文件读取
pub fn read_file_in_range(
    path: &str,
    offset: usize,
    limit: Option<usize>,
) -> Result<ReadResult> {
    let content = std::fs::read_to_string(path)?;
    let lines: Vec<&str> = content.lines().collect();
    
    let total_lines = lines.len();
    let start = offset.saturating_sub(1);
    let end = limit
        .map(|l| (start + l).min(total_lines))
        .unwrap_or(total_lines);
    
    let selected: String = lines[start..end].join("\n");
    
    Ok(ReadResult {
        content: selected,
        total_lines,
        start_line: offset,
        end_line: end,
    })
}

#[derive(Debug)]
pub struct ReadResult {
    pub content: String,
    pub total_lines: usize,
    pub start_line: usize,
    pub end_line: usize,
}
```

### 8.4 第四阶段：Token 限制检查

```rust
// crates/yode-tools/src/builtin/read_file/token_limit.rs

/// 最大 token 限制
const MAX_FILE_READ_TOKENS: usize = 100_000;

/// Token 超限错误
#[derive(Debug)]
pub struct TokenLimitExceededError {
    pub token_count: usize,
    pub max_tokens: usize,
}

impl std::fmt::Display for TokenLimitExceededError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "File content ({} tokens) exceeds maximum allowed tokens ({}). \
             Use offset and limit parameters to read specific portions.",
            self.token_count, self.max_tokens
        )
    }
}

/// 粗略 token 估算
pub fn rough_token_count(content: &str, ext: &str) -> usize {
    let factor = match ext {
        ".ts" | ".tsx" => 3.5,
        ".py" => 4,
        ".md" => 2.5,
        ".json" => 5,
        _ => 4.0,
    };
    
    (content.len() as f64 / factor).ceil() as usize
}

/// 检查 token 限制
pub fn check_token_limit(
    content: &str,
    ext: &str,
) -> Result<(), TokenLimitExceededError> {
    let tokens = rough_token_count(content, ext);
    
    if tokens > MAX_FILE_READ_TOKENS {
        return Err(TokenLimitExceededError {
            token_count: tokens,
            max_tokens: MAX_FILE_READ_TOKENS,
        });
    }
    
    Ok(())
}
```

---

## 9. 配置文件示例

```toml
# ~/.config/yode/config.toml

[tools.file_read]
# Token 限制
max_tokens = 100000

# 文件大小限制
max_size_bytes = 10485760  # 10MB

# 默认行数限制
max_lines = 2000

# PDF 设置
[tools.file_read.pdf]
max_pages_per_read = 20
extract_size_threshold_bytes = 5242880  # 5MB

# 图片设置
[tools.file_read.image]
max_width = 2048
max_height = 2048
quality = 0.85

# 阻止的设备路径
blocked_device_paths = [
    "/dev/zero", "/dev/random", "/dev/urandom",
    "/dev/stdin", "/dev/tty",
]
```

---

## 10. 总结

Claude Code FileReadTool 的核心特点：

1. **多文件类型支持** - 文本、图片、PDF、Notebook
2. **分页读取** - offset/limit 参数支持
3. **PDF 处理** - 页码范围解析、分页提取
4. **图片处理** - 自动检测格式、调整大小、token 压缩
5. **Token 限制** - 防止上下文超限
6. **安全阻止** - 阻止设备路径读取
7. **会话文件检测** - 识别 session memory/transcript 文件
8. **文件监听器** - 通知其他服务文件读取事件

Yode 优化优先级：
1. 阻止设备路径（安全）
2. 分页读取支持
3. Token 限制检查
4. 多文件类型检测
5. PDF/图片原生处理
