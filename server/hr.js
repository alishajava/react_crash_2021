// HR profile enrichment: SAML attributes first, then (optionally) a NAC REST API.
const FIELDS = ['employeeId', 'department', 'title', 'manager', 'groups'];

const DEFAULT_SAML_ATTRS = {
  employeeId: 'employeeNumber',
  department: 'department',
  title: 'title',
  manager: 'manager',
  groups: 'memberOf',
};
// Keys looked up in the NAC JSON response (dot paths allowed, e.g. "user.dept").
const DEFAULT_NAC_FIELDS = {
  employeeId: 'employeeId',
  department: 'department',
  title: 'title',
  manager: 'manager',
  groups: 'groups',
};

function parseJson(text, fallback, name) {
  if (!text) return fallback;
  try {
    return { ...fallback, ...JSON.parse(text) };
  } catch {
    throw new Error(`${name} must be valid JSON`);
  }
}

function getPath(obj, path) {
  return String(path)
    .split('.')
    .reduce((o, k) => (o === null || o === undefined ? undefined : o[k]), obj);
}

// Presets fill in env defaults for known NAC products (explicit env vars still win).
const PRESETS = {
  // Cisco ISE ERS API. Internal users only: HR fields must be stored as user custom attributes.
  'cisco-ise': {
    NAC_API_PATH: '/ers/config/internaluser/name/{id}',
    NAC_RESPONSE_ROOT: 'InternalUser',
    NAC_FIELD_MAP: JSON.stringify({
      employeeId: 'customAttributes.employeeId',
      department: 'customAttributes.department',
      title: 'customAttributes.title',
      manager: 'customAttributes.manager',
      groups: 'identityGroups', // comma-separated group IDs (GUIDs); map with NAC_GROUP_MAP
    }),
  },
};

function normalize(field, value) {
  if (value === undefined || value === null || value === '') return undefined;
  if (field === 'groups') {
    const list = Array.isArray(value) ? value : String(value).split(',');
    return list.map((g) => String(g).trim()).filter(Boolean);
  }
  return Array.isArray(value) ? String(value[0]) : String(value);
}

function pick(source, map) {
  const out = {};
  for (const f of FIELDS) {
    const v = normalize(f, getPath(source, map[f]));
    if (v !== undefined) out[f] = v;
  }
  return out;
}

function createHrResolver(rawEnv = process.env) {
  const preset = PRESETS[rawEnv.NAC_PRESET];
  if (rawEnv.NAC_PRESET && !preset) throw new Error(`Unknown NAC_PRESET: ${rawEnv.NAC_PRESET}`);
  const env = { ...(preset || {}), ...Object.fromEntries(Object.entries(rawEnv).filter(([, v]) => v)) };
  if (!env.NAC_API_URL && env.NAC_BASE_URL && env.NAC_API_PATH) {
    env.NAC_API_URL = env.NAC_BASE_URL.replace(/\/$/, '') + env.NAC_API_PATH;
  }
  const groupMap = parseJson(env.NAC_GROUP_MAP, {}, 'NAC_GROUP_MAP');
  const samlAttrs = parseJson(env.SAML_ATTR_MAP, DEFAULT_SAML_ATTRS, 'SAML_ATTR_MAP');
  const nacFields = parseJson(env.NAC_FIELD_MAP, DEFAULT_NAC_FIELDS, 'NAC_FIELD_MAP');
  const nacUrl = env.NAC_API_URL; // e.g. https://nac.corp/api/users/{id}
  const timeout = Number(env.NAC_TIMEOUT_MS || 5000);

  function nacHeaders() {
    const h = { Accept: 'application/json' };
    if (env.NAC_API_TOKEN) h.Authorization = `Bearer ${env.NAC_API_TOKEN}`;
    else if (env.NAC_API_USER) {
      h.Authorization = `Basic ${Buffer.from(`${env.NAC_API_USER}:${env.NAC_API_PASSWORD || ''}`).toString('base64')}`;
    }
    return h;
  }

  async function fetchNac(key) {
    const url = nacUrl.replace('{id}', encodeURIComponent(key));
    const res = await fetch(url, { headers: nacHeaders(), signal: AbortSignal.timeout(timeout) });
    if (!res.ok) throw new Error(`NAC API responded ${res.status}`);
    const body = await res.json();
    const root = env.NAC_RESPONSE_ROOT ? getPath(body, env.NAC_RESPONSE_ROOT) : body;
    if (!root) throw new Error('NAC response root not found');
    const out = pick(root, nacFields);
    if (out.groups) out.groups = out.groups.map((g) => groupMap[g] || g);
    return out;
  }

  // Which value identifies the user in the NAC. ISE internal users are keyed by user name.
  function defaultKey(profile) {
    let key;
    switch (env.NAC_LOOKUP_KEY) {
      case 'email': key = profile.email || profile.nameID; break;
      case 'nameID': key = profile.nameID; break;
      default: key = profile.employeeNumber || profile.email || profile.nameID; // 'auto'
    }
    if (env.NAC_STRIP_DOMAIN === 'true' && key && key.includes('@')) key = key.split('@')[0];
    return key;
  }

  // Never throws: a NAC outage must not block login (HR fields are just left as-is).
  return async function resolveHr(samlProfile, lookupKey = defaultKey(samlProfile)) {
    const hr = pick(samlProfile, samlAttrs);
    let source = 'saml';
    if (nacUrl) {
      try {
        Object.assign(hr, await fetchNac(lookupKey)); // NAC wins over SAML
        source = 'saml+nac';
      } catch (err) {
        console.warn(`[hr] NAC lookup failed for login; using SAML attributes only: ${err.message}`);
      }
    }
    return { ...hr, hrSource: source };
  };
}

module.exports = { createHrResolver, getPath };
