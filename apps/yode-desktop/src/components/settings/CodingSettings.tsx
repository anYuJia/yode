import React from "react";

import { AgentIntelligenceSettings } from "./AgentIntelligenceSettings";
import { EnvironmentsSettingsSettings as BaseEnvironmentsSettings } from "./EnvironmentsSettings";

export { HooksSettingsSettings } from "./HooksSettings";
export { GitSettingsSettings } from "./GitSettings";
export { WorktreesSettingsSettings } from "./WorktreesSettings";

export function EnvironmentsSettingsSettings({
  isZh,
  t
}: {
  isZh: boolean;
  t: (zh: string, en: string) => string;
}) {
  return (
    <>
      <AgentIntelligenceSettings isZh={isZh} t={t} />
      <BaseEnvironmentsSettings isZh={isZh} t={t} />
    </>
  );
}
