/**
 * 统一的 localStorage 读写适配层。
 *
 * 组件与 store 不再直接手写 localStorage.getItem/setItem：
 * - 读取：带类型校验与容错（损坏 JSON 回退默认值），旧键兼容读取；
 * - 写入：统一序列化；
 * - 迁移：`runStorageMigrations` 一次性执行旧值 → 新值的迁移。
 */

export type StorageMigration = {
  key: string;
  /**
   * 迁移旧值。返回 `undefined` 表示无需迁移（保留原值）；
   * 返回其他值时写回 `key`，并计入迁移数量。
   */
  migrate: (raw: string) => unknown | undefined;
};

/** 读取 JSON 值；缺失或损坏时返回 fallback，绝不抛异常。 */
export function storageReadJson<T>(key: string, fallback: T): T {
  const raw = localStorage.getItem(key);
  if (raw === null || raw === "") return fallback;
  try {
    return JSON.parse(raw) as T;
  } catch {
    return fallback;
  }
}

/** 写入 JSON 值；序列化失败时静默跳过（保留旧值），不抛异常。 */
export function storageWriteJson<T>(key: string, value: T): void {
  try {
    localStorage.setItem(key, JSON.stringify(value));
  } catch {
    // 序列化失败（如循环引用）时保留旧值
  }
}

/** 读取字符串值；缺失时返回 fallback。 */
export function storageReadString(key: string, fallback: string): string {
  const raw = localStorage.getItem(key);
  return raw === null ? fallback : raw;
}

/** 写入字符串值。 */
export function storageWriteString(key: string, value: string): void {
  try {
    localStorage.setItem(key, value);
  } catch {
    // 保留旧值
  }
}

/**
 * 执行 localStorage 迁移。每个 key 只迁移一次：迁移完成后写入
 * `yode-storage-migrations` 标记集合，后续调用跳过已迁移的 key。
 * 返回本次执行的迁移数量。
 */
export function runStorageMigrations(migrations: StorageMigration[]): number {
  const done = storageReadJson<string[]>(MIGRATIONS_DONE_KEY, []);
  let migrated = 0;
  const remaining: string[] = [];
  for (const { key, migrate } of migrations) {
    if (done.includes(key)) {
      continue;
    }
    const raw = localStorage.getItem(key);
    if (raw === null) {
      remaining.push(key);
      continue;
    }
    const next = migrate(raw);
    if (next !== undefined) {
      localStorage.setItem(key, typeof next === "string" ? next : JSON.stringify(next));
    }
    migrated += 1;
    remaining.push(key);
  }
  if (remaining.length > 0) {
    storageWriteJson(MIGRATIONS_DONE_KEY, [...done, ...remaining]);
  }
  return migrated;
}

const MIGRATIONS_DONE_KEY = "yode-storage-migrations";
