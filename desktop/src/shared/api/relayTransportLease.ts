export type RelayTransportLease = {
  wsId: number;
  generation: number;
};

type CurrentTransport = {
  wsId: number | null;
  generation: number;
};

export class RelayTransportLeaseAuthority {
  private readonly current: () => CurrentTransport;
  private readonly reset: (error: Error) => void;

  constructor(current: () => CurrentTransport, reset: (error: Error) => void) {
    this.current = current;
    this.reset = reset;
  }

  capture(): RelayTransportLease {
    const current = this.current();
    if (current.wsId === null) throw new Error("Relay is not connected.");
    return { wsId: current.wsId, generation: current.generation };
  }

  owns(lease: RelayTransportLease): boolean {
    const current = this.current();
    return (
      lease.wsId === current.wsId && lease.generation === current.generation
    );
  }

  recover(
    error: unknown,
    fallbackMessage: string,
    lease: RelayTransportLease,
  ): Error {
    const normalized =
      error instanceof Error ? error : new Error(fallbackMessage);
    if (this.owns(lease)) this.reset(normalized);
    return normalized;
  }

  async sendWithReconnectRetry(
    payload: unknown[],
    fallbackMessage: string,
    send: (payload: unknown[], lease: RelayTransportLease) => Promise<void>,
    ensureConnected: () => Promise<void>,
  ) {
    const initialLease = this.capture();
    try {
      await send(payload, initialLease);
    } catch (error) {
      const owned = this.owns(initialLease);
      const normalized = this.recover(error, fallbackMessage, initialLease);
      if (!owned) throw normalized;
      try {
        await ensureConnected();
        const retryLease = this.capture();
        try {
          await send(payload, retryLease);
        } catch (retryError) {
          throw this.recover(retryError, normalized.message, retryLease);
        }
      } catch (retryError) {
        throw retryError instanceof Error ? retryError : normalized;
      }
    }
  }
}
