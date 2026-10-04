import { DestroyRef, Injectable, NgZone, inject } from '@angular/core';
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

import type {
  AccelReport, Device, DeviceProfile, InstallFinished, InstallProgress,
  License, SetupStatus, SpaceCheck, SystemImage,
} from './models';

/** The only file in the app allowed to import from `@tauri-apps/api`. */
@Injectable({ providedIn: 'root' })
export class TauriService {
  private readonly zone = inject(NgZone);

  // ---- setup ----

  getSetupStatus(): Promise<SetupStatus> {
    return invoke<SetupStatus>('get_setup_status');
  }

  // ---- catalog ----

  listImages(): Promise<SystemImage[]> {
    return invoke<SystemImage[]>('list_images');
  }

  refreshCatalog(): Promise<SystemImage[]> {
    return invoke<SystemImage[]>('refresh_catalog');
  }

  listProfiles(): Promise<DeviceProfile[]> {
    return invoke<DeviceProfile[]>('list_profiles');
  }

  // ---- devices ----

  listDevices(): Promise<Device[]> {
    return invoke<Device[]>('list_devices');
  }

  createDevice(displayName: string, profileId: string, packagePath: string): Promise<Device> {
    return invoke<Device>('create_device', { displayName, profileId, packagePath });
  }

  updateDevice(
    name: string,
    changes: { ramMb?: number; storageMb?: number; displayName?: string },
  ): Promise<Device> {
    return invoke<Device>('update_device', { name, ...changes });
  }

  deleteDevice(name: string): Promise<void> {
    return invoke<void>('delete_device', { name });
  }

  launchDevice(name: string, coldBoot = false, wipeData = false): Promise<number> {
    return invoke<number>('launch_device', { name, coldBoot, wipeData });
  }

  stopDevice(name: string): Promise<void> {
    return invoke<void>('stop_device', { name });
  }

  // ---- licences ----

  pendingLicenses(packagePaths: string[]): Promise<License[]> {
    return invoke<License[]>('pending_licenses', { packagePaths });
  }

  /** Only ever call this from a click on an Accept button with the licence text visible. */
  acceptLicense(id: string): Promise<void> {
    return invoke<void>('accept_license', { id });
  }

  // ---- installing ----

  packagesNeededFor(packagePath: string): Promise<string[]> {
    return invoke<string[]>('packages_needed_for', { packagePath });
  }

  corePackagesNeeded(): Promise<string[]> {
    return invoke<string[]>('core_packages_needed');
  }

  startInstall(packagePaths: string[]): Promise<void> {
    return invoke<void>('start_install', { packagePaths });
  }

  /** The `install-finished` event then arrives with `cancelled: true`. */
  cancelInstall(): Promise<void> {
    return invoke<void>('cancel_install');
  }

  checkSpace(packagePaths: string[]): Promise<SpaceCheck> {
    return invoke<SpaceCheck>('check_space', { packagePaths });
  }

  // ---- acceleration ----

  checkAcceleration(): Promise<AccelReport> {
    return invoke<AccelReport>('check_acceleration');
  }

  openWindowsFeatures(): Promise<void> {
    return invoke<void>('open_windows_features');
  }

  // Tauri events arrive outside Angular's zone, so re-enter it or the view never updates.
  onInstallProgress(handler: (p: InstallProgress) => void): Promise<UnlistenFn> {
    return listen<InstallProgress>('install-progress', (event) =>
      this.zone.run(() => handler(event.payload)),
    );
  }

  onInstallFinished(handler: (r: InstallFinished) => void): Promise<UnlistenFn> {
    return listen<InstallFinished>('install-finished', (event) =>
      this.zone.run(() => handler(event.payload)),
    );
  }

  /** Subscribe and unsubscribe automatically when `destroyRef` is destroyed. */
  subscribe<T>(
    subscribeFn: (handler: (v: T) => void) => Promise<UnlistenFn>,
    handler: (v: T) => void,
    destroyRef: DestroyRef,
  ): void {
    let unlisten: UnlistenFn | null = null;
    let destroyed = false;
    destroyRef.onDestroy(() => {
      destroyed = true;
      unlisten?.();
    });
    void subscribeFn(handler).then((fn) => {
      // Destroyed before the listener finished registering: drop it straight away.
      if (destroyed) fn();
      else unlisten = fn;
    });
  }
}
