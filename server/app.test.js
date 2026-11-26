const test = require('node:test');
const assert = require('node:assert');
const http = require('node:http');
const { createApp } = require('./app');

const CERT = 'MIIBszCCAVmgAwIBAgIUdummy'; // format only; not used for login redirect
const env = {
  SAML_ENTRY_POINT: 'https://idp.example.com/sso',
  SAML_IDP_CERT: CERT,
  SESSION_SECRET: 's',
  APP_URL: 'http://localhost:3000',
};

function get(server, path) {
  return new Promise((resolve, reject) => {
    http.get({ port: server.address().port, path }, (res) => {
      let body = '';
      res.on('data', (c) => (body += c));
      res.on('end', () => resolve({ status: res.statusCode, headers: res.headers, body }));
    }).on('error', reject);
  });
}

test('env validation', () => {
  assert.throws(() => createApp({}), /Missing env/);
});

test('/api/me is 401 when logged out; /saml/login redirects to IdP; metadata served', async () => {
  const server = createApp(env).listen(0);
  try {
    assert.strictEqual((await get(server, '/api/me')).status, 401);
    const login = await get(server, '/saml/login?next=//evil.com');
    assert.strictEqual(login.status, 302);
    assert.match(login.headers.location, /^https:\/\/idp\.example\.com\/sso\?SAMLRequest=/);
    const meta = await get(server, '/saml/metadata');
    assert.match(meta.body, /AssertionConsumerService[^>]+localhost:3000\/saml\/acs/);
  } finally {
    server.close();
  }
});

// ---- HR enrichment ----
const { createHrResolver } = require('./hr');

function nacServer(handler) {
  return new Promise((resolve) => {
    const s = http.createServer(handler).listen(0, () => resolve(s));
  });
}

test('HR: SAML attrs only when NAC not configured', async () => {
  const r = createHrResolver({});
  const hr = await r({ department: 'Dev', memberOf: 'admins' }, 'a@x.com');
  assert.deepStrictEqual(hr, { department: 'Dev', groups: ['admins'], hrSource: 'saml' });
});

test('HR: NAC overrides SAML, supports root/path mapping and auth', async () => {
  let seen;
  const nac = await nacServer((req, res) => {
    seen = { url: req.url, auth: req.headers.authorization };
    res.setHeader('content-type', 'application/json');
    res.end(JSON.stringify({ result: { emp: { no: '1001' }, dept: 'HR', rank: 'Manager', mgr: 'boss', roles: ['a', 'b'] } }));
  });
  try {
    const r = createHrResolver({
      NAC_API_URL: `http://127.0.0.1:${nac.address().port}/users/{id}`,
      NAC_API_TOKEN: 'tok',
      NAC_RESPONSE_ROOT: 'result',
      NAC_FIELD_MAP: JSON.stringify({ employeeId: 'emp.no', department: 'dept', title: 'rank', manager: 'mgr', groups: 'roles' }),
    });
    const hr = await r({ department: 'OLD' }, 'a+b@x.com');
    assert.strictEqual(seen.url, '/users/a%2Bb%40x.com');
    assert.strictEqual(seen.auth, 'Bearer tok');
    assert.deepStrictEqual(hr, {
      employeeId: '1001', department: 'HR', title: 'Manager', manager: 'boss', groups: ['a', 'b'], hrSource: 'saml+nac',
    });
  } finally {
    nac.close();
  }
});

test('HR: NAC failure falls back to SAML attrs without throwing', async () => {
  const nac = await nacServer((req, res) => { res.statusCode = 500; res.end(); });
  try {
    const r = createHrResolver({ NAC_API_URL: `http://127.0.0.1:${nac.address().port}/u/{id}` });
    const warn = console.warn; console.warn = () => {};
    const hr = await r({ title: 'Eng' }, 'a@x.com');
    console.warn = warn;
    assert.deepStrictEqual(hr, { title: 'Eng', hrSource: 'saml' });
  } finally {
    nac.close();
  }
});
