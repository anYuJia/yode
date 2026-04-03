# FileEditTool 深度分析

## 1. Claude Code FileEditTool 实现

### 1.1 工具定义

```typescript
// src/tools/FileEditTool/FileEditTool.ts

export const FileEditTool = buildTool({
  name: FILE_EDIT_TOOL_NAME,
  searchHint: 'modify file contents in place',
  maxResultSizeChars: 100_000,
  strict: true,
  async description() {
    return 'A tool for editing files'
  },
  userFacingName,
  getToolUseSummary,
  getActivityDescription(input) {
    const summary = getToolUseSummary(input)
    return summary ? `Editing ${summary}` : 'Editing file'
  },
  inputSchema: () => inputSchema(),
  outputSchema: () => outputSchema(),
  toAutoClassifierInput(input) {
    return `${input.file_path}: ${input.new_string}`
  },
  getPath(input): string {
    return input.file_path
  },
  backfillObservableInput(input) {
    // 将相对路径扩展为绝对路径
    if (typeof input.file_path === 'string') {
      input.file_path = expandPath(input.file_path)
    }
  },
  async preparePermissionMatcher({ file_path }) {
    return pattern => matchWildcardPattern(pattern, file_path)
  },
  async checkPermissions(input, context): Promise<PermissionDecision> {
    const appState = context.getAppState()
    return checkWritePermissionForTool(
      FileEditTool,
      input,
      appState.toolPermissionContext,
    )
  },
  // ... 更多实现
})
```

### 1.2 输入/输出 Schema

```typescript
// src/tools/FileEditTool/types.ts

const inputSchema = lazySchema(() =>
  z.strictObject({
    file_path: z.string().describe('文件路径'),
    old_string: z.string().describe('要替换的原始内容'),
    new_string: z.string().describe('新的内容'),
    replace_all: z.boolean().optional().describe('是否替换所有匹配项'),
  }),
)

const outputSchema = lazySchema(() =>
  z.object({
    filePath: z.string(),
    oldString: z.string(),
    newString: z.string(),
    originalFile: z.string(),
    structuredPatch: z.array(z.object({
      type: z.enum(['insert', 'delete', 'replace']),
      line: z.number(),
      content: z.string(),
    })),
    userModified: z.boolean(),
    replaceAll: z.boolean(),
    gitDiff: z.object({
      unified: z.string(),
      added: z.number(),
      deleted: z.number(),
    }).optional(),
  }),
)
```

### 1.3 validateInput 验证流程

```typescript
async validateInput(input: FileEditInput, toolUseContext: ToolUseContext) {
  const { file_path, old_string, new_string, replace_all = false } = input
  const fullFilePath = expandPath(file_path)

  // 1. 检查是否引入 secrets（team memory 文件）
  const secretError = checkTeamMemSecrets(fullFilePath, new_string)
  if (secretError) {
    return { result: false, message: secretError, errorCode: 0 }
  }

  // 2. 检查是否有实际变化
  if (old_string === new_string) {
    return {
      result: false,
      behavior: 'ask',
      message: 'No changes to make: old_string and new_string are exactly the same.',
      errorCode: 1,
    }
  }

  // 3. 检查权限规则（deny）
  const denyRule = matchingRuleForInput(
    fullFilePath,
    appState.toolPermissionContext,
    'edit',
    'deny',
  )
  if (denyRule !== null) {
    return {
      result: false,
      behavior: 'ask',
      message: 'File is in a directory that is denied by your permission settings.',
      errorCode: 2,
    }
  }

  // 4. UNC 路径安全检查（防止 NTLM 凭证泄露）
  if (fullFilePath.startsWith('\\\\') || fullFilePath.startsWith('//')) {
    return { result: true }
  }

  // 5. 文件大小检查（防止 OOM）
  const MAX_EDIT_FILE_SIZE = 1024 * 1024 * 1024 // 1 GiB
  try {
    const { size } = await fs.stat(fullFilePath)
    if (size > MAX_EDIT_FILE_SIZE) {
      return {
        result: false,
        behavior: 'ask',
        message: `File is too large to edit (${formatFileSize(size)}).`,
        errorCode: 10,
      }
    }
  } catch (e) {
    if (!isENOENT(e)) throw e
  }

  // 6. 读取文件内容（检测编码）
  let fileContent: string | null = null
  try {
    const fileBuffer = await fs.readFileBytes(fullFilePath)
    const encoding: BufferEncoding =
      fileBuffer.length >= 2 &&
      fileBuffer[0] === 0xff &&
      fileBuffer[1] === 0xfe
        ? 'utf16le'  // BOM 检测
        : 'utf8'
    fileContent = fileBuffer.toString(encoding).replaceAll('\r\n', '\n')
  } catch (e) {
    if (isENOENT(e)) {
      fileContent = null
    } else {
      throw e
    }
  }

  // 7. 文件不存在的处理
  if (fileContent === null) {
    if (old_string === '') {
      return { result: true }  // 允许创建空文件
    }
    const similarFilename = findSimilarFile(fullFilePath)
    const cwdSuggestion = await suggestPathUnderCwd(fullFilePath)
    let message = `File does not exist. ${FILE_NOT_FOUND_CWD_NOTE} ${getCwd()}.`
    if (cwdSuggestion) {
      message += ` Did you mean ${cwdSuggestion}?`
    } else if (similarFilename) {
      message += ` Did you mean ${similarFilename}?`
    }
    return {
      result: false,
      behavior: 'ask',
      message,
      errorCode: 4,
    }
  }

  // 8. 空 old_string 检查
  if (old_string === '') {
    if (fileContent.trim() !== '') {
      return {
        result: false,
        behavior: 'ask',
        message: 'Cannot create new file - file already exists.',
        errorCode: 3,
      }
    }
    return { result: true }  // 空文件允许写入
  }

  // 9. Notebook 文件检查
  if (fullFilePath.endsWith('.ipynb')) {
    return {
      result: false,
      behavior: 'ask',
      message: `File is a Jupyter Notebook. Use the ${NOTEBOOK_EDIT_TOOL_NAME} to edit this file.`,
      errorCode: 5,
    }
  }

  // 10. 读取状态检查（确保先读后写）
  const readTimestamp = toolUseContext.readFileState.get(fullFilePath)
  if (!readTimestamp || readTimestamp.isPartialView) {
    return {
      result: false,
      behavior: 'ask',
      message: 'File has not been read yet. Read it first before writing to it.',
      errorCode: 6,
    }
  }

  // 11. 文件修改检查（compare mtime）
  if (readTimestamp) {
    const lastWriteTime = getFileModificationTime(fullFilePath)
    if (lastWriteTime > readTimestamp.timestamp) {
      const isFullRead =
        readTimestamp.offset === undefined &&
        readTimestamp.limit === undefined
      // 如果是完整读取，比较内容作为 fallback
      if (isFullRead && fileContent === readTimestamp.content) {
        // 内容未变，安全
      } else {
        return {
          result: false,
          behavior: 'ask',
          message: 'File has been modified since read...',
          errorCode: 7,
        }
      }
    }
  }

  // 12. 查找实际的 old_string（处理引号规范化）
  const actualOldString = findActualString(fileContent, old_string)
  if (!actualOldString) {
    return {
      result: false,
      behavior: 'ask',
      message: `String to replace not found in file.`,
      errorCode: 8,
    }
  }

  // 13. 多重匹配检查
  const matches = fileContent.split(actualOldString).length - 1
  if (matches > 1 && !replace_all) {
    return {
      result: false,
      behavior: 'ask',
      message: `Found ${matches} matches...`,
      errorCode: 9,
    }
  }

  // 14. Settings 文件验证
  const settingsValidationResult = validateInputForSettingsFileEdit(...)
  if (settingsValidationResult !== null) {
    return settingsValidationResult
  }

  return { result: true, meta: { actualOldString } }
}
```

### 1.4 call 执行流程

```typescript
async call(input: FileEditInput, context) {
  const { file_path, old_string, new_string, replace_all = false } = input

  // 1. 路径扩展
  const absoluteFilePath = expandPath(file_path)

  // 2. 技能发现（fire-and-forget）
  if (!isEnvTruthy(process.env.CLAUDE_CODE_SIMPLE)) {
    const newSkillDirs = await discoverSkillDirsForPaths([absoluteFilePath], cwd)
    for (const dir of newSkillDirs) {
      dynamicSkillDirTriggers?.add(dir)
    }
    addSkillDirectories(newSkillDirs).catch(() => {})
    activateConditionalSkillsForPaths([absoluteFilePath], cwd)
  }

  // 3. 诊断追踪（LSP）
  await diagnosticTracker.beforeFileEdited(absoluteFilePath)

  // 4. 文件历史备份
  if (fileHistoryEnabled()) {
    await fileHistoryTrackEdit(
      updateFileHistoryState,
      absoluteFilePath,
      parentMessage.uuid,
    )
  }

  // 5. 确保父目录存在
  await fs.mkdir(dirname(absoluteFilePath))

  // 6. 读取当前文件内容
  const { content: originalFileContents, fileExists, encoding, lineEndings } = 
    readFileForEdit(absoluteFilePath)

  // 7. 再次检查文件修改状态
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

  // 8. 引号样式处理
  const actualOldString = findActualString(originalFileContents, old_string) || old_string
  const actualNewString = preserveQuoteStyle(old_string, actualOldString, new_string)

  // 9. 生成 patch
  const { patch, updatedFile } = getPatchForEdit({
    filePath: absoluteFilePath,
    fileContents: originalFileContents,
    oldString: actualOldString,
    newString: actualNewString,
    replaceAll: replace_all,
  })

  // 10. 写入磁盘
  writeTextContent(absoluteFilePath, updatedFile, encoding, endings)

  // 11. 通知 LSP 服务器
  const lspManager = getLspServerManager()
  if (lspManager) {
    clearDeliveredDiagnosticsForFile(`file://${absoluteFilePath}`)
    // didChange: 内容已修改
    lspManager.changeFile(absoluteFilePath, updatedFile).catch(...)
    // didSave: 文件已保存
    lspManager.saveFile(absoluteFilePath).catch(...)
  }

  // 12. 通知 VSCode（diff 视图）
  notifyVscodeFileUpdated(absoluteFilePath, originalFileContents, updatedFile)

  // 13. 更新读取状态
  readFileState.set(absoluteFilePath, {
    content: updatedFile,
    timestamp: getFileModificationTime(absoluteFilePath),
    offset: undefined,
    limit: undefined,
  })

  // 14. 事件日志
  if (absoluteFilePath.endsWith(`${sep}CLAUDE.md`)) {
    logEvent('tengu_write_claudemd', {})
  }
  countLinesChanged(patch)
  logFileOperation({ operation: 'edit', tool: 'FileEditTool', filePath: absoluteFilePath })
  logEvent('tengu_edit_string_lengths', {
    oldStringBytes: Buffer.byteLength(old_string, 'utf8'),
    newStringBytes: Buffer.byteLength(new_string, 'utf8'),
    replaceAll: replace_all,
  })

  // 15. Git diff（如果启用）
  let gitDiff: ToolUseDiff | undefined
  if (isEnvTruthy(process.env.CLAUDE_CODE_REMOTE) && 
      getFeatureValue_CACHED_MAY_BE_STALE('tengu_quartz_lantern', false)) {
    const startTime = Date.now()
    const diff = await fetchSingleFileGitDiff(absoluteFilePath)
    if (diff) gitDiff = diff
    logEvent('tengu_tool_use_diff_computed', {
      isEditTool: true,
      durationMs: Date.now() - startTime,
      hasDiff: !!diff,
    })
  }

  // 16. 返回结果
  return {
    data: {
      filePath: file_path,
      oldString: actualOldString,
      newString: new_string,
      originalFile: originalFileContents,
      structuredPatch: patch,
      userModified: userModified ?? false,
      replaceAll: replace_all,
      ...(gitDiff && { gitDiff }),
    },
  }
}
```

### 1.5 关键工具函数

```typescript
// src/tools/FileEditTool/utils.ts

/**
 * 查找实际的字符串（处理引号规范化）
 */
function findActualString(file: string, searchString: string): string | null {
  // 尝试 exact match
  if (file.includes(searchString)) {
    return searchString
  }

  // 尝试规范化引号后匹配
  const normalizedSearch = searchString
    .replace(/'/g, '"')
    .replace(/`/g, '"')

  if (file.includes(normalizedSearch)) {
    return normalizedSearch
  }

  // 尝试模糊匹配（前后各扩展 50 字符）
  // ...

  return null
}

/**
 * 保留引号样式
 */
function preserveQuoteStyle(
  oldString: string,
  actualOldString: string,
  newString: string,
): string {
  // 检测文件使用的引号类型
  const fileUsesCurlyQuotes = actualOldString.includes('"'') || actualOldString.includes('"')
  
  if (fileUsesCurlyQuotes && !newString.includes('"'')) {
    // 将新字符串的直引号替换为弯引号
    return newString
      .replace(/"/g, '"')
      .replace(/'/g, '\'')
  }
  
  return newString
}

/**
 * 生成 patch
 */
function getPatchForEdit(options: {
  filePath: string
  fileContents: string
  oldString: string
  newString: string
  replaceAll: boolean
}): { patch: StructuredPatch; updatedFile: string } {
  const { fileContents, oldString, newString, replaceAll } = options

  // 执行替换
  const updatedFile = replaceAll
    ? fileContents.replaceAll(oldString, newString)
    : fileContents.replace(oldString, newString)

  // 生成 structured patch
  const patch = createStructuredPatch(
    options.filePath,
    options.filePath,
    fileContents,
    updatedFile,
  )

  return { patch, updatedFile }
}
```

---

## 2. Yode 实现建议

### 2.1 Rust 实现

```rust
// crates/yode-tools/src/builtin/edit_file.rs

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

/// 文件编辑输入
#[derive(Debug, Clone, Deserialize)]
pub struct EditFileInput {
    pub file_path: String,
    pub old_string: String,
    pub new_string: String,
    #[serde(default)]
    pub replace_all: bool,
}

/// 文件编辑输出
#[derive(Debug, Clone, Serialize)]
pub struct EditFileOutput {
    pub file_path: String,
    pub old_string: String,
    pub new_string: String,
    pub original_file: String,
    pub structured_patch: Vec<PatchEntry>,
    pub user_modified: bool,
    pub replace_all: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct PatchEntry {
    #[serde(rename = "type")]
    pub patch_type: String,  // "insert", "delete", "replace"
    pub line: usize,
    pub content: String,
}

/// 文件编辑工具
pub struct EditFileTool {
    /// 最大文件大小（1 GiB）
    max_file_size: u64,
}

impl EditFileTool {
    pub fn new() -> Self {
        Self {
            max_file_size: 1024 * 1024 * 1024, // 1 GiB
        }
    }

    /// 验证输入
    pub async fn validate_input(
        &self,
        input: &EditFileInput,
        read_state: &ReadState,
    ) -> ValidationResult {
        let full_path = expand_path(&input.file_path);

        // 1. 检查是否有实际变化
        if input.old_string == input.new_string {
            return ValidationResult::reject(
                "No changes to make: old_string and new_string are exactly the same.",
            );
        }

        // 2. 检查文件大小
        if let Ok(metadata) = fs::metadata(&full_path) {
            if metadata.len() > self.max_file_size {
                return ValidationResult::reject(format!(
                    "File is too large to edit ({} bytes). Maximum is {} bytes.",
                    metadata.len(),
                    self.max_file_size
                ));
            }
        }

        // 3. 读取文件内容
        let file_content = match fs::read_to_string(&full_path) {
            Ok(content) => Some(content),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return ValidationResult::reject(format!("Cannot read file: {}", e)),
        };

        // 4. 文件不存在的处理
        let file_content = match file_content {
            Some(content) => content,
            None => {
                if input.old_string.is_empty() {
                    return ValidationResult::ok(); // 允许创建空文件
                }
                return ValidationResult::reject(format!(
                    "File does not exist: {}. Did you mean {}?",
                    full_path.display(),
                    suggest_similar_file(&full_path)
                ));
            }
        };

        // 5. 空 old_string 检查
        if input.old_string.is_empty() {
            if !file_content.trim().is_empty() {
                return ValidationResult::reject(
                    "Cannot create new file - file already exists.".to_string(),
                );
            }
            return ValidationResult::ok();
        }

        // 6. Notebook 文件检查
        if full_path.extension().map_or(false, |ext| ext == "ipynb") {
            return ValidationResult::reject(
                "File is a Jupyter Notebook. Use notebook_edit instead.".to_string(),
            );
        }

        // 7. 读取状态检查
        if !read_state.has_read(&full_path) {
            return ValidationResult::reject(
                "File has not been read yet. Read it first before writing to it.".to_string(),
            );
        }

        // 8. 文件修改检查
        if let Some(read_info) = read_state.get(&full_path) {
            if let Ok(mtime) = get_file_mtime(&full_path) {
                if mtime > read_info.timestamp {
                    // 内容比较 fallback
                    if read_info.content != file_content {
                        return ValidationResult::reject(
                            "File has been modified since read.".to_string(),
                        );
                    }
                }
            }
        }

        // 9. 查找实际的 old_string
        let actual_old = find_actual_string(&file_content, &input.old_string)
            .ok_or_else(|| format!("String to replace not found: {}", input.old_string))?;

        // 10. 多重匹配检查
        let matches = file_content.split(&actual_old).count() - 1;
        if matches > 1 && !input.replace_all {
            return ValidationResult::reject(format!(
                "Found {} matches, but replace_all is false.",
                matches
            ));
        }

        ValidationResult::ok_with_meta(ValidationMeta {
            actual_old_string: actual_old,
        })
    }

    /// 执行文件编辑
    pub async fn execute(
        &self,
        input: EditFileInput,
        context: &ToolContext,
    ) -> ToolResult {
        let full_path = expand_path(&input.file_path);

        // 1. 确保父目录存在
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent)
                .context("Failed to create parent directories")?;
        }

        // 2. 读取原始内容
        let original_content = fs::read_to_string(&full_path)
            .unwrap_or_default();

        // 3. 查找实际字符串（处理引号）
        let actual_old = find_actual_string(&original_content, &input.old_string)
            .unwrap_or_else(|| input.old_string.clone());
        
        let actual_new = preserve_quote_style(&input.old_string, &actual_old, &input.new_string);

        // 4. 执行替换
        let updated_content = if input.replace_all {
            original_content.replace(&actual_old, &actual_new)
        } else {
            original_content.replacen(&actual_old, &actual_new, 1)
        };

        // 5. 写入磁盘
        fs::write(&full_path, &updated_content)
            .context("Failed to write file")?;

        // 6. 生成 patch
        let patch = generate_patch(&original_content, &updated_content);

        // 7. 更新读取状态
        context.read_state.set(&full_path, ReadInfo {
            content: updated_content.clone(),
            timestamp: get_file_mtime(&full_path).unwrap_or_default(),
        });

        // 8. 返回结果
        ToolResult::success(EditFileOutput {
            file_path: input.file_path,
            old_string: actual_old,
            new_string: input.new_string,
            original_file: original_content,
            structured_patch: patch,
            user_modified: false,
            replace_all: input.replace_all,
        })
    }
}

/// 辅助函数：查找实际字符串
fn find_actual_string(file: &str, search: &str) -> Option<String> {
    // Exact match
    if file.contains(search) {
        return Some(search.to_string());
    }

    // Normalize quotes and try again
    let normalized = search
        .replace('\'', "\"")
        .replace('`', "\"");
    
    if file.contains(&normalized) {
        return Some(normalized);
    }

    None
}

/// 辅助函数：保留引号样式
fn preserve_quote_style(old: &str, actual_old: &str, new: &str) -> String {
    let uses_curly = actual_old.contains('"') || actual_old.contains(''');
    
    if uses_curly && !new.contains('"') {
        return new
            .replace('"', """)
            .replace('\'', '\'');
    }
    
    new.to_string()
}

/// 辅助函数：生成结构化 patch
fn generate_patch(original: &str, updated: &str) -> Vec<PatchEntry> {
    let original_lines: Vec<&str> = original.lines().collect();
    let updated_lines: Vec<&str> = updated.lines().collect();

    // 使用 diff 算法（如 myers diff）
    // 这里简化实现
    vec![]
}

fn get_file_mtime(path: &Path) -> Result<u64> {
    fs::metadata(path)?
        .modified()
        .map(|t| t.duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as u64)
        .context("Failed to get file modification time")
}
```

---

## 3. 关键设计要点

### 3.1 安全性检查清单

| 检查项 | 目的 | 实现位置 |
|--------|------|----------|
| 文件大小限制 | 防止 OOM | validateInput #4 |
| 文件存在性 | 友好错误 | validateInput #7 |
| 读取状态验证 | 确保先读后写 | validateInput #10 |
| _mtime_ 检查 | 检测外部修改 | validateInput #11 |
| 多重匹配检查 | 避免意外替换 | validateInput #13 |
| UNC 路径检查 | 防止 NTLM 泄露 | validateInput #4 |
| Secrets 检查 | 防止密钥泄露 | validateInput #1 |

### 3.2 用户体验优化

1. **智能错误消息** - 提供具体的错误码和建议
2. **相似文件建议** - 文件不存在时提供替代选项
3. **引号规范化** - 自动处理直引号/弯引号
4. **Git diff 集成** - 自动计算变更统计

### 3.3 性能优化

1. **lazySchema** - 延迟加载 Schema，减少启动时间
2. **fire-and-forget 技能发现** - 不阻塞主流程
3. **LSP 异步通知** - 后台通知，不阻塞写入

---

## 4. 总结

Claude Code 的 FileEditTool 是一个高度工程化的实现，核心特点：

1. **严格的验证** - 14 步验证流程
2. **安全第一** - 多层安全检查
3. **用户体验** - 智能错误消息和 fallback
4. **生态集成** - LSP、VSCode、Git 深度集成

Yode 可以借鉴的关键点：
- 文件修改时间检查（防止竞态条件）
- 引号样式保留（提升用户体验）
- 结构化 patch 生成（便于审计）
- Git diff 集成（变更追踪）
