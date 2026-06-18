export const LLM_PROVIDERS_STORAGE_KEY = "yode-llm-providers";

export type ProviderModelsMeta = {
  id: string;
  defaultModels: string[];
};

type StoredProvider = {
  id: string;
  models: string[];
};

export function lastModelStorageKey(provider: string) {
  return `yode-last-model-${provider}`;
}

export function parseStoredProviders(raw: string | null): StoredProvider[] {
  if (!raw) return [];
  try {
    const data = JSON.parse(raw);
    const list = Array.isArray(data) ? data : Object.values(data);
    return list
      .filter(isStoredProvider)
      .map((provider) => ({
        id: provider.id,
        models: provider.models.filter((model) => typeof model === "string")
      }));
  } catch {
    return [];
  }
}

export function modelsForProvider(
  provider: string,
  rawStoredProviders: string | null,
  providerMeta: ProviderModelsMeta[]
) {
  const stored = parseStoredProviders(rawStoredProviders).find((item) => item.id === provider);
  if (stored && stored.models.length > 0) return stored.models;
  return providerMeta.find((item) => item.id === provider)?.defaultModels ?? [];
}

export function preferredModelForProvider(
  provider: string,
  rawStoredProviders: string | null,
  providerMeta: ProviderModelsMeta[],
  lastUsedModel: string | null
) {
  const models = modelsForProvider(provider, rawStoredProviders, providerMeta);
  if (lastUsedModel && models.includes(lastUsedModel)) return lastUsedModel;
  return models[0] || "";
}

function isStoredProvider(value: unknown): value is StoredProvider {
  if (!value || typeof value !== "object") return false;
  const provider = value as Record<string, unknown>;
  return typeof provider.id === "string" && Array.isArray(provider.models);
}
