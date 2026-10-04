# 14 — Milestone, Testing and Polish

**Goal:** confirm Phase 3 is genuinely finished, close the gaps you left behind, and know
what Phase 4 is for.

---

## 14.1 The milestone

Phase 3 is complete when **someone who has never installed Android Studio can create and
boot an Android emulator using only your app.**

Test it properly, on a clean state. Move your SDK aside first:

```bash
mv ~/Library/Application\ Support/avdhub ~/Desktop/avdhub-backup
mv ~/.android/avd/AvdHub_Test_Device.avd ~/Desktop/ 2>/dev/null
rm -f ~/.android/avd/AvdHub_Test_Device.ini
```

Then run `npm run tauri dev` and walk the whole path as a new user would:

1. The setup screen appears (no emulator installed).
2. It states your machine, the install location, and the download size.
3. "Get started" shows the licence, then a progress bar that moves.
4. It lands on an empty devices screen.
5. "Create your first device" → name, type, Android version with its size.
6. Licence → download → device appears in the list.
7. Start → amber dot → green dot → an Android home screen.
8. Stop → grey dot.
9. Delete → confirmation → gone.

**Do this with someone watching who is not a developer.** Every place they hesitate is a
bug, even if the code is correct. Write down what they ask; those notes are your Phase 4
backlog.

Restore your backup afterwards if you want the downloads back:

```bash
rm -rf ~/Library/Application\ Support/avdhub
mv ~/Desktop/avdhub-backup ~/Library/Application\ Support/avdhub
```

---

## 14.2 Gaps to close before you call it done

These were deliberately left in the previous documents.

**1. The setup screen auto-accepts licences.** Fix it: give the setup screen the same
licence step the create screen has. This is the only item on this list that is not optional
— it is the rule from document 06.

**2. `packages_needed_for("")` pushes an empty string.** Guard it as shown in §13.5, or add
a dedicated `core_packages_needed` command and have the setup screen call that instead. The
second is cleaner.

**3. No cancel button on downloads.** Correct for now (the engine cannot cancel), but the
UI should at least explain that closing the window will interrupt an install. One line of
text.

**4. The device list polls even when the window is hidden.** Wasteful and, on a laptop,
noticeable. Pause polling on `document.visibilitychange`:

```ts
// in DevicesComponent constructor
document.addEventListener('visibilitychange', () => {
  if (document.hidden) this.store.stopPolling();
  else { void this.store.refresh(); this.store.startPolling(); }
});
```
Remember to remove the listener on destroy.

**5. Errors are raw engine strings.** "file system problem at /Users/…: Permission denied
(os error 13)" is honest but not friendly. A small mapping layer in the UI — match on a few
known phrases, fall back to the raw text — goes a long way. Keep the original available
behind a "details" toggle rather than discarding it.

---

## 14.3 Testing what is now testable

**The engine's 12 tests still run and still matter.** Nothing in Phase 3 touched it. If
`cargo test -p engine` is red, you changed something you should not have.

**Components are testable because the service is injectable.** A mock is a plain object:

```ts
// src/app/devices/devices.component.spec.ts
import { TestBed } from '@angular/core/testing';

import { DevicesComponent } from './devices.component';
import { TauriService } from '../core/tauri.service';
import type { Device } from '../core/models';

const device: Device = {
  name: 'Test_Device', displayName: 'Test Device', apiLevel: 35,
  androidName: 'Android 15', abi: 'arm64-v8a', tag: 'Google Play',
  ramMb: 2048, width: 1080, height: 2400, playStore: true,
  running: false, booted: false, serial: null,
};

describe('DevicesComponent', () => {
  beforeEach(() => {
    TestBed.configureTestingModule({
      imports: [DevicesComponent],
      providers: [
        {
          provide: TauriService,
          useValue: {
            listDevices: () => Promise.resolve([device]),
            launchDevice: () => Promise.resolve(1234),
            stopDevice: () => Promise.resolve(),
            deleteDevice: () => Promise.resolve(),
          },
        },
      ],
    });
  });

  it('lists devices', async () => {
    const fixture = TestBed.createComponent(DevicesComponent);
    fixture.detectChanges();
    await fixture.whenStable();
    fixture.detectChanges();
    expect(fixture.nativeElement.textContent).toContain('Test Device');
  });
});
```

No Tauri, no Rust, no emulator — which is the payoff of the boundary rule from document 12.
Write a handful of these for the logic that matters: the status label, the create-device
state machine, the empty state.

**Add the frontend job to CI** if you have not already (from the Dependabot conversation):

```yaml
  frontend:
    name: frontend
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with:
          node-version: lts/*
          cache: npm
      - run: npm ci
      - run: npm run build
      - run: npm test -- --watch=false --browsers=ChromeHeadless
```

Now an npm update that breaks Angular fails a check instead of surprising you.

---

## 14.4 The first release

You have a working app. Tag it:

```bash
git tag v0.1.0
git push --tags
```

The release workflow from document 09 builds `.dmg` and `.msi` files and attaches them to a
draft release. Check the artifacts, write two sentences of notes, publish.

**Warn people in the release notes** that the builds are unsigned: macOS will say the app
is from an unidentified developer (right-click → Open to bypass) and Windows SmartScreen
will interrupt (More info → Run anyway). Better to explain it than to let them assume the
worst. Signing is a Phase 5 concern — an Apple Developer account is $99/year and a Windows
certificate has its own cost, and neither is worth paying before anyone uses the thing.

---

## 14.5 What Phase 4 is for

Your Windows code has now been *compiled* by CI many times and *run* never. That is the
whole subject of Phase 4:

- **Hypervisor detection and guidance.** The number one Windows failure: no WHPX/Hyper-V,
  so the emulator either refuses or crawls. Detect it (`emulator -accel-check`, registry,
  `systeminfo`) and walk the user through enabling it. This is the feature that makes your
  app better than the alternatives rather than merely smaller.
- **Real Windows testing.** A cheap second-hand PC beats a VM: nested virtualisation makes
  emulator testing in a VM unusable.
- **Path and process edge cases.** Long paths, spaces in usernames, the console-window flag
  you wrote blind in document 08.
- **Intel vs Apple Silicon** on real hardware for both.

Get the Phase 3 milestone signed off by a real person first. A polished flow that works on
one platform is worth more than a half-finished flow on two.

---

## 14.6 Final checkpoint

- [ ] Clean-state walkthrough completes without touching a terminal.
- [ ] A non-developer completed it without you intervening.
- [ ] `cargo test -p engine` green; `cargo check` green in `src-tauri`.
- [ ] `npm run build` succeeds.
- [ ] CI green on all runners including the frontend job.
- [ ] The setup screen shows licence text before accepting.
- [ ] `v0.1.0` tagged and the release artifacts build.

```bash
git add -A && git commit -m "Phase 3 complete: working GUI"
```

---

## 14.7 Understanding check

1. Why does moving `~/Library/Application Support/avdhub` aside give a better test than
   reading the code?
2. Why can component tests run without Rust, Tauri, or an emulator?
3. Why is the auto-accepted licence the only mandatory item in §14.2?
4. Why is a second-hand Windows PC better than a VM for Phase 4?
