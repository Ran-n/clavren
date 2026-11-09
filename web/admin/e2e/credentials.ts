// The admin's sign-in codes. The server's clock stands still, so its authenticator accepts only
// three steps (the frozen one and either side), each once and in increasing order; recovery codes
// also work once each. What has been used is kept in a file, so a restarted test worker never
// reuses a code.

import { createHmac } from "node:crypto";
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import type { ServerInfo, ServerSpec } from "./servers";

const STEP_SECONDS = 30;
const BASE32 = "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

type State = { secret: string; lastStep: number; recoveryCodes: string[]; otherRecoveryCodes: string[] };

function base32Decode(text: string): Buffer {
  let bits = "";
  for (const character of text.replace(/=+$/, "").toUpperCase()) {
    const value = BASE32.indexOf(character);
    if (value < 0) throw new Error(`not Base32: ${character}`);
    bits += value.toString(2).padStart(5, "0");
  }
  const bytes = bits.match(/.{8}/g) ?? [];
  return Buffer.from(bytes.map((byte) => parseInt(byte, 2)));
}

/** RFC 6238: HMAC-SHA-1, 30-second steps, 6 digits. */
export function totpAt(secret: string, step: number): string {
  const counter = Buffer.alloc(8);
  counter.writeBigUInt64BE(BigInt(step));
  const digest = createHmac("sha1", base32Decode(secret)).update(counter).digest();
  const offset = (digest[digest.length - 1] as number) & 0x0f;
  const binary = digest.readUInt32BE(offset) & 0x7fffffff;
  return String(binary % 1_000_000).padStart(6, "0");
}

export class Credentials {
  readonly info: ServerInfo;
  private readonly frozenStep: number;

  constructor(private readonly spec: ServerSpec) {
    this.info = JSON.parse(readFileSync(spec.infoFile, "utf8")) as ServerInfo;
    this.frozenStep = Math.floor(Date.parse(this.info.frozen_at) / 1000 / STEP_SECONDS);
  }

  private load(): State {
    if (existsSync(this.spec.stateFile)) return JSON.parse(readFileSync(this.spec.stateFile, "utf8")) as State;
    // The admin was enrolled an hour before the frozen time: every step around it is still unused.
    return {
      secret: this.info.secret,
      lastStep: 0,
      recoveryCodes: [...this.info.recovery_codes],
      otherRecoveryCodes: [...(this.info.other?.recovery_codes ?? [])],
    };
  }

  private save(state: State): void {
    writeFileSync(this.spec.stateFile, JSON.stringify(state));
  }

  /** The next authenticator code the server will accept. */
  totp(): string {
    const state = this.load();
    const step = Math.max(state.lastStep + 1, this.frozenStep - 1);
    if (step > this.frozenStep + 1) throw new Error("no authenticator code left for the frozen clock; use a recovery code");
    this.save({ ...state, lastStep: step });
    return totpAt(state.secret, step);
  }

  /** A code for `secret` before it is confirmed; confirming it records its step as used. */
  confirmNewSecret(secret: string): string {
    const step = this.frozenStep - 1;
    this.save({ ...this.load(), secret, lastStep: step });
    return totpAt(secret, step);
  }

  recoveryCode(): string {
    const state = this.load();
    const [code, ...rest] = state.recoveryCodes;
    if (!code) throw new Error("no recovery code left");
    this.save({ ...state, recoveryCodes: rest });
    return code;
  }

  /** A recovery code of the second admin (the flows server only). */
  otherRecoveryCode(): string {
    const state = this.load();
    const [code, ...rest] = state.otherRecoveryCodes;
    if (!code) throw new Error("no recovery code left for the other admin (only the flows server has one)");
    this.save({ ...state, otherRecoveryCodes: rest });
    return code;
  }

  replaceRecoveryCodes(codes: string[]): void {
    this.save({ ...this.load(), recoveryCodes: codes });
  }
}
