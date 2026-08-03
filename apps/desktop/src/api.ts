import { invoke } from '@tauri-apps/api/core';
import type {
  Node,
  Scaffold,
  AdvisorRequest,
  AdvisorResponse,
  Plan,
  UndoEntry,
  CondaEnv,
  SteamInventory,
  WorkshopItem,
} from './types';
import { isTauri } from './env';
import * as mocks from './mocks';

export const api = {
  scan: (path: string) =>
    isTauri ? invoke<Node>('scan_path', { path }) : mocks.scan(path),

  listScaffolds: () =>
    isTauri ? invoke<Scaffold[]>('list_scaffolds') : Promise.resolve(mocks.SCAFFOLDS),

  detectScaffold: (path: string) =>
    isTauri ? invoke<string | null>('detect_scaffold', { path }) : mocks.detectScaffold(path),

  scopeSizes: (
    scaffoldId: string,
    rootPath: string,
    scopeDays?: Record<string, number>,
    wxidFilter?: string[],
    envFilter?: string[],
  ) =>
    isTauri
      ? invoke<{ scope_id: string; bytes: number; file_count: number; total_bytes: number; total_files: number }[]>('scope_sizes', {
          scaffoldId,
          rootPath,
          scopeDays: scopeDays ?? null,
          wxidFilter: wxidFilter ?? null,
          envFilter: envFilter ?? null,
        })
      : mocks.scopeSizes(scaffoldId, rootPath),

  executeScope: (
    scaffoldId: string,
    scopeId: string,
    rootPath: string,
    dryRun: boolean,
    olderThanDays?: number,
    wxidFilter?: string[],
    envFilter?: string[],
  ) =>
    isTauri
      ? invoke<UndoEntry[]>('execute_scope', {
          scaffoldId,
          scopeId,
          rootPath,
          dryRun,
          olderThanDays: olderThanDays ?? null,
          wxidFilter: wxidFilter ?? null,
          envFilter: envFilter ?? null,
        })
      : Promise.resolve([] as UndoEntry[]),

  listCondaEnvs: (condaRoot: string) =>
    isTauri
      ? invoke<CondaEnv[]>('list_conda_envs', { condaRoot })
      : Promise.resolve([] as CondaEnv[]),

  advise: (req: AdvisorRequest) =>
    isTauri ? invoke<AdvisorResponse>('advise', { req }) : mocks.advise(req),

  inspect: (path: string, sampleCount: number) =>
    isTauri ? invoke<string[]>('inspect_path', { path, sampleCount }) : mocks.inspect(path, sampleCount),

  revealInExplorer: (path: string) =>
    isTauri ? invoke<void>('reveal_in_explorer', { path }) : Promise.resolve(),

  execute: (plan: Plan, dryRun: boolean) =>
    isTauri ? invoke<UndoEntry[]>('execute_plan', { plan, dryRun }) : mocks.execute(plan, dryRun),

  volumeInfo: (path: string) =>
    isTauri
      ? invoke<{ total_bytes: number; used_bytes: number; free_bytes: number }>('volume_info', { path })
      : Promise.resolve(null),

  estimateSize: (path: string) =>
    isTauri ? invoke<number>('estimate_size', { path }) : Promise.resolve(0),

  setAdvisor: (
    provider: 'openai' | 'atlas' | 'anthropic' | 'gemini' | 'ollama',
    model: string,
    apiKey?: string,
    baseUrl?: string,
  ) =>
    isTauri
      ? invoke<void>('set_advisor', {
          provider,
          apiKey: apiKey ?? null,
          model,
          baseUrl: baseUrl ?? null,
        })
      : Promise.resolve(),

  listSteamGames: () =>
    isTauri
      ? invoke<SteamInventory>('list_steam_games')
      : Promise.resolve(mocks.STEAM_INVENTORY),

  listSteamWorkshopItems: (libraryRoot: string, appid: number) =>
    isTauri
      ? invoke<WorkshopItem[]>('list_steam_workshop_items', { libraryRoot, appid })
      : Promise.resolve(mocks.steamWorkshopItems(appid)),

  fetchWorkshopTitles: (ids: number[]) =>
    isTauri
      ? invoke<Record<number, string>>('fetch_workshop_titles', { ids })
      : Promise.resolve(mocks.workshopTitles(ids)),

  openSteamUrl: (
    action: 'uninstall' | 'rungameid' | 'validate' | 'nav' | 'workshop_page',
    appid: number,
  ) =>
    isTauri ? invoke<void>('open_steam_url', { action, appid }) : Promise.resolve(),
};
