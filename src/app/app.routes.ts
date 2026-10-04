import type { Routes } from '@angular/router';

export const routes: Routes = [
  { path: '', pathMatch: 'full', redirectTo: 'devices' },
  {
    path: 'devices',
    loadComponent: () => import('./devices/devices.component').then((m) => m.DevicesComponent),
  },
  {
    path: 'devices/new',
    loadComponent: () =>
      import('./create-device/create-device.component').then((m) => m.CreateDeviceComponent),
  },
  {
    path: 'setup',
    loadComponent: () => import('./setup/setup.component').then((m) => m.SetupComponent),
  },
];
