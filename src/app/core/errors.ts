/** A friendlier sentence for a raw engine error, keeping the original for a "details" toggle. */
export interface FriendlyError {
  message: string;
  details: string;
}

const KNOWN: Array<[RegExp, string]> = [
  [/network problem/i, 'Could not reach Google’s download servers. Check your internet connection and try again.'],
  [/permission denied|os error 13/i, 'The app does not have permission to write to the install folder.'],
  [/no space left|os error 28/i, 'There is not enough free disk space for this download.'],
  [/corrupted/i, 'The download was damaged on the way. Please try again.'],
  [/already exists/i, 'A device with that name already exists. Pick another name.'],
  [/not installed yet/i, 'That Android version has not been downloaded yet.'],
  [/licence must be accepted/i, 'Google’s licence has to be accepted before downloading.'],
];

export function friendlyError(raw: unknown): FriendlyError {
  const details = String(raw);
  const match = KNOWN.find(([pattern]) => pattern.test(details));
  return { message: match ? match[1] : details, details };
}
