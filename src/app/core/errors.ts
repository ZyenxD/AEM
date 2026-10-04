/** A human sentence for a raw engine error. The original is kept for "Technical details". */
export interface FriendlyError {
  message: string;
  suggestion: string | null;
  technical: string;
}

// Add rules only for failures actually seen; a hypothetical rule tends to match the wrong thing.
const RULES: Array<{ match: RegExp; message: string; suggestion: string | null }> = [
  {
    match: /no space left|not enough space|os error 28|os error 112/i,
    message: 'Your disk is full.',
    suggestion: 'Free up a few gigabytes and try again.',
  },
  {
    match: /permission denied|access is denied|os error 13|os error 5\b/i,
    message: "AEM doesn't have permission to write there.",
    suggestion: 'If you use antivirus software, it may be blocking the download.',
  },
  {
    match: /network problem|dns|timed out|connection refused/i,
    message: "Couldn't reach Google's servers.",
    suggestion: 'Check your internet connection and try again.',
  },
  {
    match: /corrupted|checksum/i,
    message: 'The download was damaged in transit.',
    suggestion: 'This usually fixes itself — try again.',
  },
  {
    match: /licence must be accepted|license must be accepted/i,
    message: "Google's licence needs to be accepted first.",
    suggestion: null,
  },
  {
    match: /already exists/i,
    message: 'A device with that name already exists.',
    suggestion: 'Choose a different name.',
  },
  {
    match: /android version .* is not installed yet/i,
    message: "This device's Android version isn't downloaded on this computer.",
    suggestion: 'Create a new device with an installed Android version, or install it with Android Studio.',
  },
  {
    match: /closed right after starting/i,
    message: 'The emulator closed right after starting.',
    suggestion: 'Open Technical details for the reason.',
  },
  {
    match: /not installed yet|emulator package/i,
    message: 'The Android emulator still needs to be installed.',
    suggestion: 'Finish setup first.',
  },
  {
    match: /is not running/i,
    message: 'That device has already stopped.',
    suggestion: null,
  },
  {
    match: /you need about .* mb free/i,
    message: 'There is not enough free disk space for this download.',
    suggestion: 'Free up some space and try again.',
  },
];

export function friendly(error: unknown): FriendlyError {
  const technical = String(error);
  const rule = RULES.find((r) => r.match.test(technical));
  return {
    message: rule?.message ?? 'Something went wrong.',
    suggestion: rule?.suggestion ?? null,
    technical,
  };
}
