import { recordFromUnknown } from "./jsonUtils";

export const LLM_PROVIDERS_STORAGE_KEY = "yode-llm-providers";
export const LLM_PROVIDERS_CHANGE_EVENT = "yode-llm-providers-change";
export const DEFAULT_LLM_CHANGE_EVENT = "yode-default-llm-change";

export type DefaultLlmChangeDetail = {
  provider: string;
  model: string;
};

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
    window.dispatchEvent(new Event(LLM_PROVIDERS_CHANGE_EVENT));
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

export function parseStoredProviderValues(raw: string | null): unknown[] {
  if (!raw) return [];
  try {
    const data: unknown = JSON.parse(raw);
    if (Array.isArray(data)) {
      return data;
    }
    if (!data || typeof data !== "object") {
      return [];
    }
    return Object.entries(data).map(([id, value]) => {
      if (value && typeof value === "object") {
        return { id, ...value };
      }
      return { id };
    });
  } catch {
    return [];
  }
}

export function loadStoredProviderValues() {
  return parseStoredProviderValues(loadStoredProvidersRaw());
}

export function parseDefaultLlmChangeDetail(detail: unknown): DefaultLlmChangeDetail | null {
  const record = recordFromUnknown(detail);
  if (!record) return null;
  return typeof record.provider === "string" && typeof record.model === "string"
    ? { provider: record.provider, model: record.model }
    : null;
}

export function detailFromDefaultLlmChangeEvent(event: Event): DefaultLlmChangeDetail | null {
  return event instanceof CustomEvent ? parseDefaultLlmChangeDetail(event.detail) : null;
}

export function dispatchDefaultLlmChange(detail: unknown) {
  if (typeof window !== "undefined") {
    window.dispatchEvent(new CustomEvent(DEFAULT_LLM_CHANGE_EVENT, { detail }));
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

export function providerOptionsFromStoredProviders(providerMeta: Array<{ id: string; nameEn: string }>) {
  return providerOptionsFromStorage(loadStoredProvidersRaw(), providerMeta);
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

export function providerDisplayNameFromStorage(
  providerId: string,
  providerMeta: Array<{ id: string; name: string }>
) {
  return providerDisplayName(providerId, loadStoredProvidersRaw(), providerMeta);
}

export function modelsForProviderFromStorage(
  provider: string,
  providerMeta: ProviderModelsMeta[]
) {
  return modelsForProvider(provider, loadStoredProvidersRaw(), providerMeta);
}

function isStoredProvider(value: unknown): value is StoredProvider {
  const provider = recordFromUnknown(value);
  if (!provider) return false;
  return typeof provider.id === "string" && Array.isArray(provider.models);
}
