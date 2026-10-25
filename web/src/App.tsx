import { useCallback, useEffect, useMemo, useState } from 'react';
import type { SubmitEvent } from 'react';
import { ApiError, createApi } from './api';
import type { SessionUser } from './api';
import { Redirects } from './components/Redirects';
import { PasswordForm, Users } from './components/Users';
import { ErrorMessage, formValue } from './components/shared';

function Login({
	onLogin,
	message,
}: {
	onLogin: (token: string, user: SessionUser) => void;
	message: string;
}) {
	const [error, setError] = useState<unknown>(null);
	const [busy, setBusy] = useState(false);
	async function submit(event: SubmitEvent<HTMLFormElement>) {
		event.preventDefault();
		setBusy(true);
		setError(null);
		const form = event.currentTarget;
		try {
			const data = await createApi().login({
				username: formValue(form, 'username'),
				password: formValue(form, 'password'),
			});
			onLogin(data.token, data.user);
		}
		catch (err) {
			setError(err);
		}
		finally {
			setBusy(false);
		}
	}
	return (
		<main className="mx-auto mt-16 max-w-sm px-4">
			<h1 className="text-2xl font-semibold tracking-tight">JustGoto</h1>
			<p className="mt-2 mb-6 muted">Sign in to manage your redirects.</p>
			{message && (
				<p role="status" className="mb-4 muted">
					{message}
				</p>
			)}
			<form onSubmit={submit} className="panel grid gap-4">
				<label className="field">
					Username
					<input name="username" autoComplete="username" required autoFocus />
				</label>
				<label className="field">
					Password
					<input
						name="password"
						type="password"
						autoComplete="current-password"
						required
					/>
				</label>
				<ErrorMessage error={error} />
				<button className="button primary" disabled={busy}>
					{busy ? 'Signing in…' : 'Sign in'}
				</button>
			</form>
			<a className="mt-5 inline-block muted underline" href="/swagger-ui/">
				API documentation
			</a>
		</main>
	);
}

export function App() {
	const [token, setToken] = useState(() => sessionStorage.getItem('justgoto-token') || '');
	const [user, setUser] = useState<SessionUser | null>(null);
	const [checking, setChecking] = useState(Boolean(token));
	const [sessionError, setSessionError] = useState<unknown>(null);
	const [message, setMessage] = useState('');
	const [tab, setTab] = useState<'redirects' | 'users' | 'account'>('redirects');
	const [signingOut, setSigningOut] = useState(false);
	const clearSession = useCallback((notice = '') => {
		sessionStorage.removeItem('justgoto-token');
		setToken('');
		setUser(null);
		setTab('redirects');
		setMessage(notice);
	}, []);
	const api = useMemo(
		() =>
			createApi(token, () => clearSession('Your session has expired. Please sign in again.')),
		[token, clearSession],
	);
	const refreshUser = useCallback(async () => {
		setSessionError(null);
		try {
			const account = await api.me();
			setUser(account);
			if (!account.roles.includes('admin')) {
				setTab((current) => (current === 'users' ? 'redirects' : current));
			}
		}
		catch (error) {
			if (!(error instanceof ApiError && error.status === 401)) {
				setSessionError(error);
			}
		}
	}, [api]);
	useEffect(() => {
		if (!token) {
			setChecking(false);
			return;
		}
		const controller = new AbortController();
		setSessionError(null);
		api.me(controller.signal)
			.then((account) => {
				if (!controller.signal.aborted) setUser(account);
			})
			.catch((error) => {
				if (
					!controller.signal.aborted &&
					!(error instanceof ApiError && error.status === 401)
				) {
					setSessionError(error);
				}
			})
			.finally(() => {
				if (!controller.signal.aborted) setChecking(false);
			});
		return () => controller.abort();
	}, [token, api]);
	async function logout() {
		setSigningOut(true);
		setSessionError(null);
		try {
			await api.logout();
			clearSession();
		}
		catch (error) {
			setSessionError(error);
		}
		finally {
			setSigningOut(false);
		}
	}
	if (checking) {
		return (
			<p role="status" className="p-8 muted">
				Loading your account…
			</p>
		);
	}
	if (token && !user && sessionError) {
		return (
			<main className="mx-auto mt-16 grid max-w-sm gap-4 px-4">
				<ErrorMessage error={sessionError} />
				<button className="button" onClick={() => window.location.reload()}>
					Retry
				</button>
				<button className="button" onClick={() => clearSession()}>
					Back to sign in
				</button>
			</main>
		);
	}
	if (!token || !user) {
		return (
			<Login
				message={message}
				onLogin={(sessionToken, sessionUser) => {
					sessionStorage.setItem('justgoto-token', sessionToken);
					setToken(sessionToken);
					setUser(sessionUser);
					setMessage('');
				}}
			/>
		);
	}
	const admin = user.roles.includes('admin');
	const tabs: ('redirects' | 'users' | 'account')[] = [
		'redirects',
		...(admin ? ['users' as const] : []),
		'account',
	];
	return (
		<div className="mx-auto max-w-5xl px-4 py-6 sm:px-6">
			<header className="flex flex-wrap items-center justify-between gap-4 border-b border-slate-200 pb-5">
				<a href="/" className="text-xl font-semibold tracking-tight">
					JustGoto
				</a>
				<div className="flex flex-wrap items-center gap-4">
					<span className="muted">
						{user.username} · {user.roles.join(', ') || 'No role'}
					</span>
					<a className="underline" href="/swagger-ui/">
						API docs
					</a>
					<button className="button" onClick={logout} disabled={signingOut}>
						{signingOut ? 'Signing out…' : 'Sign out'}
					</button>
				</div>
			</header>
			<nav aria-label="Main" className="my-6 flex gap-2">
				{tabs.map((name) => (
					<button
						key={name}
						aria-current={tab === name ? 'page' : undefined}
						className={`button capitalize ${tab === name ? 'primary' : ''}`}
						onClick={() => setTab(name)}
					>
						{name}
					</button>
				))}
			</nav>
			<main>
				<ErrorMessage error={sessionError} />
				{tab === 'redirects' && (
					<Redirects api={api} canEdit={admin || user.roles.includes('editor')} />
				)}
				{tab === 'users' && admin && (
					<Users
						api={api}
						currentUser={user}
						onSelfUpdated={refreshUser}
						onSessionRevoked={() =>
							clearSession('Your account changed. Please sign in again.')
						}
					/>
				)}
				{tab === 'account' && (
					<div className="grid max-w-md gap-5">
						<h1 className="text-xl font-semibold">Account</h1>
						<div className="panel">
							<PasswordForm
								api={api}
								user={user}
								self
								onChanged={() =>
									clearSession('Password updated. Please sign in again.')
								}
							/>
						</div>
					</div>
				)}
			</main>
		</div>
	);
}
