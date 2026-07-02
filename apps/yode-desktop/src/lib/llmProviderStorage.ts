export const LLM_PROVIDERS_STORAGE_KEY = "yode-llm-providers";

export type ProviderModelsMeta = {
  id: string;
  defaultModels: string[];
};

type StoredProvider = {
  id: string;
  name?: string;
  enabled?: boolean;
  models: string[];
};

export function lastModelStorageKey(provider: string) {
  return `yode-last-model-${provider}`;
}

export function loadStoredProvidersRaw() {
  return localStorage.getItem(LLM_PROVIDERS_STORAGE_KEY);
}

export function saveStoredProviders(providers: unknown[]) {
  localStorage.setItem(LLM_PROVIDERS_STORAGE_KEY, JSON.stringify(providers));
  if (typeof window !== "undefined") {
    window.dispatchEvent(new Event("yode-llm-providers-change"));
  }
}

export function loadLastModelForProvider(provider: string) {
  return localStorage.getItem(lastModelStorageKey(provider));
}

export function saveLastModelForProvider(provider: string, model: string) {
  localStorage.setItem(lastModelStorageKey(provider), model);
}

export function parseStoredProviders(raw: string | null): StoredProvider[] {
  if (!raw) return [];
  try {
    const data = JSON.parse(raw);
    const list = Array.isArray(data) ? data : Object.values(data);
    return list
      .filter(isStoredProvider)
      .map((provider) => {
        const parsed: StoredProvider = {
          id: provider.id,
          models: provider.models.filter((model) => typeof model === "string")
        };
        if (typeof provider.name === "string") parsed.name = provider.name;
        if (typeof provider.enabled === "boolean") parsed.enabled = provider.enabled;
        return parsed;
      });
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

export function preferredModelFromStorage(
  provider: string,
  providerMeta: ProviderModelsMeta[]
) {
  return preferredModelForProvider(
    provider,
    loadStoredProvidersRaw(),
    providerMeta,
    loadLastModelForProvider(provider)
  );
}

export function providerOptionsFromStorage(
  rawStoredProviders: string | null,
  providerMeta: Array<{ id: string; nameEn: string }>
) {
  const enabledProviders = parseStoredProviders(rawStoredProviders).filter((provider) => provider.enabled);
  if (enabledProviders.length === 0) {
    return providerMeta.map((provider) => ({
      value: provider.id,
      label: provider.nameEn
    }));
  }
  return enabledProviders.map((provider) => ({
    value: provider.id,
    label: provider.name || provider.id
  }));
}

export function providerDisplayName(
  providerId: string,
  rawStoredProviders: string | null,
  providerMeta: Array<{ id: string; name: string }>
) {
  const stored = parseStoredProviders(rawStoredProviders).find((provider) => provider.id === providerId);
  if (stored?.name) return stored.name;
  return providerMeta.find((provider) => provider.id === providerId)?.name || providerId;
}

function isStoredProvider(value: unknown): value is StoredProvider {
  if (!value || typeof value !== "object") return false;
  const provider = value as Record<string, unknown>;
  return typeof provider.id === "string" && Array.isArray(provider.models);
}
