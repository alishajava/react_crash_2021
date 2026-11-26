import { useEffect, useState } from 'react';
import logo from './logo.svg';
import './App.css';

function App() {
  const [auth, setAuth] = useState({ loading: true, user: null });

  useEffect(() => {
    fetch('/api/me', { credentials: 'include' })
      .then((r) => (r.ok ? r.json() : { user: null }))
      .then((d) => setAuth({ loading: false, user: d.user || null }))
      .catch(() => setAuth({ loading: false, user: null }));
  }, []);

  const login = () => {
    window.location.href = `/saml/login?next=${encodeURIComponent(window.location.pathname)}`;
  };
  const logout = async () => {
    await fetch('/api/logout', { method: 'POST', credentials: 'include' });
    setAuth({ loading: false, user: null });
  };

  if (auth.loading) return <div className="App">Loading…</div>;

  const failed = new URLSearchParams(window.location.search).get('sso') === 'failed';

  return (
    <div className="App">
      <header className="App-header">
        <img src={logo} className="App-logo" alt="logo" />
        {auth.user ? (
          <>
            <p>Signed in as {auth.user.name} ({auth.user.email})</p>
            <ul style={{ fontSize: '1rem', textAlign: 'left' }}>
              {[
                ['Employee ID', auth.user.employeeId],
                ['Department', auth.user.department],
                ['Title', auth.user.title],
                ['Manager', auth.user.manager],
                ['Groups', (auth.user.groups || []).join(', ')],
              ]
                .filter(([, v]) => v)
                .map(([k, v]) => (
                  <li key={k}>{k}: {v}</li>
                ))}
            </ul>
            <button onClick={logout}>Sign out</button>
          </>
        ) : (
          <>
            {failed && <p role="alert">SSO login failed. Please try again.</p>}
            <button onClick={login}>Sign in with SSO</button>
          </>
        )}
      </header>
    </div>
  );
}

export default App;
