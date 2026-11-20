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
