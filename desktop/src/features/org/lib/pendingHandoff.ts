type PendingHandoff = {
  executor: string;
  requiresParent: boolean;
};

let pending: PendingHandoff | null = null;

export function setPendingHandoffExecutor(
  pubkey: string | null,
  requiresParent = false,
): void {
  pending = pubkey
    ? { executor: pubkey.trim().toLowerCase(), requiresParent }
    : null;
}

export function peekPendingHandoffExecutor(): string | null {
  return pending?.executor ?? null;
}

export function takePendingHandoffExecutor(): string | null {
  return takePendingHandoff()?.executor ?? null;
}

export function peekPendingHandoff(): PendingHandoff | null {
  return pending;
}

export function takePendingHandoff(): PendingHandoff | null {
  const next = pending;
  pending = null;
  return next;
}
