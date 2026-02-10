/**
 * Passkey Authentication Client for Exemem Desktop
 *
 * WebAuthn-based passkey authentication adapted for the Tauri desktop client.
 * All API calls use absolute URLs (takes apiBaseUrl parameter).
 */

// Base64URL encoding/decoding utilities
function base64UrlEncode(buffer) {
  const bytes = new Uint8Array(buffer);
  let binary = "";
  for (let i = 0; i < bytes.length; i++) {
    binary += String.fromCharCode(bytes[i]);
  }
  return btoa(binary)
    .replace(/\+/g, "-")
    .replace(/\//g, "_")
    .replace(/=+$/, "");
}

function base64UrlDecode(str) {
  let base64 = str.replace(/-/g, "+").replace(/_/g, "/");
  const padding = base64.length % 4;
  if (padding) {
    base64 += "=".repeat(4 - padding);
  }
  const binary = atob(base64);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) {
    bytes[i] = binary.charCodeAt(i);
  }
  return bytes;
}

// Convert server creation options to browser PublicKeyCredentialCreationOptions
function convertCreationOptions(serverOptions) {
  const publicKey = serverOptions.publicKey;

  return {
    challenge: base64UrlDecode(publicKey.challenge),
    rp: publicKey.rp,
    user: {
      id: base64UrlDecode(publicKey.user.id),
      name: publicKey.user.name,
      displayName: publicKey.user.displayName || publicKey.user.name,
    },
    pubKeyCredParams: publicKey.pubKeyCredParams,
    timeout: publicKey.timeout || 60000,
    attestation: publicKey.attestation || "none",
    authenticatorSelection: {
      authenticatorAttachment:
        publicKey.authenticatorSelection?.authenticatorAttachment || "platform",
      residentKey:
        publicKey.authenticatorSelection?.residentKey || "preferred",
      userVerification:
        publicKey.authenticatorSelection?.userVerification || "preferred",
    },
    excludeCredentials: (publicKey.excludeCredentials || []).map((cred) => ({
      id: base64UrlDecode(cred.id),
      type: cred.type,
      transports: cred.transports,
    })),
  };
}

// Convert server request options to browser PublicKeyCredentialRequestOptions
function convertRequestOptions(serverOptions) {
  const publicKey = serverOptions.publicKey;

  return {
    challenge: base64UrlDecode(publicKey.challenge),
    timeout: publicKey.timeout || 60000,
    rpId: publicKey.rpId,
    allowCredentials: (publicKey.allowCredentials || []).map((cred) => ({
      id: base64UrlDecode(cred.id),
      type: cred.type,
      transports: cred.transports,
    })),
    userVerification: publicKey.userVerification || "preferred",
  };
}

/**
 * Check if WebAuthn is supported in this environment
 */
export function isWebAuthnSupported() {
  return !!(
    window.PublicKeyCredential &&
    typeof window.PublicKeyCredential === "function"
  );
}

/**
 * Register a new passkey
 * @param {string} apiBaseUrl - Full API base URL (e.g. https://ygyu7ritx8.execute-api.us-west-2.amazonaws.com)
 * @returns {{ ok: boolean, userHash?: string, sessionToken?: string, error?: string }}
 */
export async function registerPasskey(apiBaseUrl) {
  // Start registration
  const startRes = await fetch(`${apiBaseUrl}/auth/passkey/register/start`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({}),
  });
  const startData = await startRes.json();

  if (!startData?.ok || !startData.options) {
    return {
      ok: false,
      error: startData?.error || "Failed to start registration",
    };
  }

  const challenge = startData.options.publicKey?.challenge;
  const creationOptions = convertCreationOptions(startData.options);

  // Browser credential ceremony (Touch ID, etc.)
  let credential;
  try {
    credential = await navigator.credentials.create({
      publicKey: creationOptions,
    });
  } catch (err) {
    return { ok: false, error: err.message || "User cancelled registration" };
  }

  if (!credential) {
    return { ok: false, error: "User cancelled registration" };
  }

  // Finish registration
  const attestationResponse = credential.response;
  const credentialData = {
    id: credential.id,
    rawId: base64UrlEncode(credential.rawId),
    type: credential.type,
    response: {
      clientDataJSON: base64UrlEncode(attestationResponse.clientDataJSON),
      attestationObject: base64UrlEncode(
        attestationResponse.attestationObject,
      ),
    },
  };

  const finishRes = await fetch(`${apiBaseUrl}/auth/passkey/register/finish`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ challenge, credential: credentialData }),
  });
  const finishData = await finishRes.json();

  if (!finishData?.ok) {
    return { ok: false, error: finishData?.error || "Registration failed" };
  }

  return {
    ok: true,
    userHash: finishData.user_hash,
    sessionToken: finishData.session_token,
  };
}

/**
 * Sign in with an existing passkey
 * @param {string} apiBaseUrl - Full API base URL
 * @returns {{ ok: boolean, userHash?: string, sessionToken?: string, error?: string }}
 */
export async function loginPasskey(apiBaseUrl) {
  // Start authentication
  const startRes = await fetch(`${apiBaseUrl}/auth/passkey/login/start`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({}),
  });
  const startData = await startRes.json();

  if (!startData?.ok || !startData.options) {
    return {
      ok: false,
      error: startData?.error || "Failed to start authentication",
    };
  }

  const challenge = startData.options.publicKey?.challenge;
  const requestOptions = convertRequestOptions(startData.options);

  // Browser credential ceremony
  let credential;
  try {
    credential = await navigator.credentials.get({
      publicKey: requestOptions,
    });
  } catch (err) {
    return {
      ok: false,
      error: err.message || "User cancelled authentication",
    };
  }

  if (!credential) {
    return { ok: false, error: "User cancelled authentication" };
  }

  // Finish authentication
  const assertionResponse = credential.response;
  const credentialData = {
    id: credential.id,
    rawId: base64UrlEncode(credential.rawId),
    type: credential.type,
    response: {
      clientDataJSON: base64UrlEncode(assertionResponse.clientDataJSON),
      authenticatorData: base64UrlEncode(assertionResponse.authenticatorData),
      signature: base64UrlEncode(assertionResponse.signature),
      userHandle: assertionResponse.userHandle
        ? base64UrlEncode(assertionResponse.userHandle)
        : null,
    },
  };

  const finishRes = await fetch(`${apiBaseUrl}/auth/passkey/login/finish`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ challenge, credential: credentialData }),
  });
  const finishData = await finishRes.json();

  if (!finishData?.ok) {
    return {
      ok: false,
      error: finishData?.error || "Authentication failed",
    };
  }

  return {
    ok: true,
    userHash: finishData.user_hash,
    sessionToken: finishData.session_token,
  };
}

/**
 * Create an API key using an authenticated session
 * @param {string} apiBaseUrl - Full API base URL
 * @param {string} sessionToken - Bearer token from passkey auth
 * @param {string} userHash - User hash from passkey auth
 * @returns {{ ok: boolean, apiKey?: string, error?: string }}
 */
export async function createApiKey(apiBaseUrl, sessionToken, userHash) {
  const res = await fetch(`${apiBaseUrl}/api/developer/api-keys`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      Authorization: `Bearer ${sessionToken}`,
      "X-User-Hash": userHash,
    },
    body: JSON.stringify({ name: "Exemem Desktop Client" }),
  });
  const data = await res.json();

  if (!res.ok || data.error) {
    return {
      ok: false,
      error: data?.error || `API key creation failed (${res.status})`,
    };
  }

  // The API returns the key in data.api_key or data.key
  const apiKey = data.api_key || data.key;
  if (!apiKey) {
    return { ok: false, error: "No API key returned from server" };
  }

  return { ok: true, apiKey };
}
