import { render, screen } from '@testing-library/react';
import App from './App';

afterEach(() => jest.restoreAllMocks());

test('shows SSO sign-in button when logged out', async () => {
  global.fetch = jest.fn().mockResolvedValue({ ok: false, json: async () => ({}) });
  render(<App />);
  expect(await screen.findByText(/sign in with sso/i)).toBeInTheDocument();
});

test('shows user when logged in', async () => {
  global.fetch = jest.fn().mockResolvedValue({
    ok: true,
    json: async () => ({ user: { name: 'Ann', email: 'a@x.com' } }),
  });
  render(<App />);
  expect(await screen.findByText(/signed in as ann/i)).toBeInTheDocument();
});
