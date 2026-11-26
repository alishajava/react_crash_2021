const express = require('express');
const session = require('express-session');
const passport = require('passport');
const { Strategy: SamlStrategy } = require('@node-saml/passport-saml');
const { createHrResolver } = require('./hr');

function createApp(env = process.env) {
  const required = ['SAML_ENTRY_POINT', 'SAML_IDP_CERT', 'SESSION_SECRET'];
  const missing = required.filter((k) => !env[k]);
  if (missing.length) throw new Error(`Missing env: ${missing.join(', ')}`);

  const baseUrl = env.APP_URL || 'http://localhost:3000'; // browser-facing origin (CRA proxies /api, /saml)
  const resolveHr = createHrResolver(env);
  const issuer = env.SAML_ISSUER || `${baseUrl}/saml/metadata`;

  passport.use(
    new SamlStrategy(
      {
        callbackUrl: `${baseUrl}/saml/acs`,
        entryPoint: env.SAML_ENTRY_POINT,
        issuer,
        idpCert: env.SAML_IDP_CERT,
        wantAssertionsSigned: true,
        wantAuthnResponseSigned: false,
        identifierFormat: null,
      },
      (profile, done) => {
        const email = profile.email || profile.nameID;
        resolveHr(profile)
          .then((hr) =>
            done(null, {
              id: profile.nameID,
              email,
              name: profile.displayName || profile.cn || profile.email || profile.nameID,
              ...hr,
            })
          )
          .catch(done);
      },
      (profile, done) => done(null, profile) // logout verify
    )
  );
  passport.serializeUser((u, done) => done(null, u));
  passport.deserializeUser((u, done) => done(null, u));

  const app = express();
  app.use(
    session({
      secret: env.SESSION_SECRET,
      resave: false,
      saveUninitialized: false,
      cookie: { httpOnly: true, sameSite: 'lax', secure: env.NODE_ENV === 'production' },
    })
  );
  app.use(express.urlencoded({ extended: false }));
  app.use(passport.initialize());
  app.use(passport.session());

  // Only allow same-site relative redirects after login (prevent open redirect)
  const safeNext = (n) => (typeof n === 'string' && /^\/(?!\/)/.test(n) ? n : '/');

  app.get('/saml/login', (req, res, next) => {
    req.session.next = safeNext(req.query.next);
    passport.authenticate('saml', { failureRedirect: '/?sso=failed' })(req, res, next);
  });

  app.post('/saml/acs', passport.authenticate('saml', { failureRedirect: '/?sso=failed' }), (req, res) => {
    const next = safeNext(req.session.next);
    delete req.session.next;
    res.redirect(next);
  });

  app.get('/saml/metadata', (req, res) => {
    const strategy = passport._strategy('saml');
    res.type('application/xml').send(strategy.generateServiceProviderMetadata(null, null));
  });

  app.get('/api/me', (req, res) => {
    if (!req.isAuthenticated()) return res.status(401).json({ authenticated: false });
    res.json({ authenticated: true, user: req.user });
  });

  app.post('/api/logout', (req, res, next) => {
    req.logout((err) => {
      if (err) return next(err);
      req.session.destroy(() => res.json({ ok: true }));
    });
  });

  return app;
}

module.exports = { createApp };
