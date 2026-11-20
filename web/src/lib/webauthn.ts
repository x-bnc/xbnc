import { api, type Me } from "./api";

export const passkeysSupported = typeof window !== "undefined" && "PublicKeyCredential" in window && window.isSecureContext;

function fromB64url(s: string): ArrayBuffer {
  const b = atob(s.replace(/-/g, "+").replace(/_/g, "/") + "=".repeat((4 - (s.length % 4)) % 4));
  return Uint8Array.from(b, (c) => c.charCodeAt(0)).buffer;
}

function toB64url(buf: ArrayBuffer): string {
  let s = "";
  for (const b of new Uint8Array(buf)) s += String.fromCharCode(b);
  return btoa(s).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

interface Options {
  id: string;
  publicKey: Record<string, unknown> & { challenge: string };
}

/** Creates a passkey on this device and stores it on the server. */
export async function registerPasskey(label: string): Promise<void> {
  const o = await api.post<Options>("/me/passkeys/options");
  const pk = o.publicKey as Record<string, any>;
  const cred = (await navigator.credentials.create({
    publicKey: {
      ...pk,
      challenge: fromB64url(pk.challenge),
      user: { ...pk.user, id: fromB64url(pk.user.id) },
      excludeCredentials: (pk.excludeCredentials ?? []).map((c: { id: string }) => ({ type: "public-key", id: fromB64url(c.id) })),
    } as PublicKeyCredentialCreationOptions,
  })) as PublicKeyCredential | null;
  if (!cred) throw new Error("cancelled");
  const r = cred.response as AuthenticatorAttestationResponse;
  await api.post("/me/passkeys", {
    id: o.id,
    label,
    client_data_json: toB64url(r.clientDataJSON),
    attestation_object: toB64url(r.attestationObject),
  });
}

/** Signs in with a passkey saved on this device or a security key. */
export async function signInWithPasskey(): Promise<Me> {
  const o = await api.post<Options>("/auth/passkey/options");
  const pk = o.publicKey as Record<string, any>;
  const cred = (await navigator.credentials.get({
    publicKey: { ...pk, challenge: fromB64url(pk.challenge), allowCredentials: [] } as PublicKeyCredentialRequestOptions,
  })) as PublicKeyCredential | null;
  if (!cred) throw new Error("cancelled");
  const r = cred.response as AuthenticatorAssertionResponse;
  return api.post<Me>("/auth/passkey", {
    id: o.id,
    credential_id: toB64url(cred.rawId),
    client_data_json: toB64url(r.clientDataJSON),
    authenticator_data: toB64url(r.authenticatorData),
    signature: toB64url(r.signature),
  });
}

/** True when the user closed the browser's passkey window. */
export function wasCancelled(e: unknown): boolean {
  const n = (e as { name?: string })?.name;
  return n === "NotAllowedError" || n === "AbortError" || (e as Error)?.message === "cancelled";
}
