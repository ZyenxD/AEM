// Mirrors src-tauri/src/dto.rs. camelCase because of #[serde(rename_all = "camelCase")];
// `| null` because Rust's Option<T> serialises to null. Keep the two files in sync.

export interface Device {
  name: string;
  displayName: string;
  apiLevel: number | null;
  androidName: string | null;
  abi: string | null;
  tag: string | null;
  ramMb: number | null;
  width: number | null;
  height: number | null;
  playStore: boolean;
  running: boolean;
  booted: boolean;
  serial: string | null;
}

export interface SystemImage {
  packagePath: string;
  apiLevel: number | null;
  androidName: string;
  tagId: string | null;
  tagLabel: string | null;
  sizeMb: number;
  installed: boolean;
  recommended: boolean;
}

export interface DeviceProfile {
  id: string;
  label: string;
  width: number;
  height: number;
  ramMb: number;
}

export interface License {
  id: string;
  text: string;
}

export interface SetupStatus {
  sdkRoot: string;
  managedByUs: boolean;
  hasEmulator: boolean;
  hasAdb: boolean;
  installedImageCount: number;
  abiLabel: string;
  osLabel: string;
  freeDiskGb: number | null;
  ready: boolean;
  platformWarning: string | null;
}

export type InstallStage = 'downloading' | 'verifying' | 'unpacking' | 'finished';

export interface InstallProgress {
  packagePath: string;
  label: string;
  index: number;
  total: number;
  stage: InstallStage;
  doneBytes: number;
  totalBytes: number;
  percent: number;
}

export interface InstallFinished {
  ok: boolean;
  cancelled: boolean;
  error: string | null;
}

export interface SpaceCheck {
  neededMb: number;
  availableMb: number | null;
  enough: boolean;
}

// Mirrors engine/src/accel.rs (serialised as camelCase there).
export type AccelStatus =
  | 'ready' | 'hypervisorDisabled' | 'virtualizationDisabled'
  | 'emulatorMissing' | 'unknown';

export interface FixStep {
  title: string;
  detail: string;
  command: string | null;
  requiresAdmin: boolean;
}

export interface AccelReport {
  status: AccelStatus;
  summary: string;
  rawMessage: string;
  fixSteps: FixStep[];
  requiresReboot: boolean;
}
