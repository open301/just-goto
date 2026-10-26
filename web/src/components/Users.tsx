import { useCallback, useState } from 'react';
import type { SubmitEvent } from 'react';
import type { Api, User, UserDraft, SessionUser } from '../api';
import { ErrorMessage, Pager, PAGE_SIZE, formValue, useList } from './shared';

const emptyUser: UserDraft = { username: '', role: 'viewer' };

/** Handle both personal password changes and admin password resets. */
export function PasswordForm({
	api,
	user,
	self,
	onChanged,
}: {
	api: Api;
	user: SessionUser;
	self: boolean;
	onChanged?: () => void;
}) {
	const [error, setError] = useState<unknown>(null);
	const [message, setMessage] = useState('');
	const [busy, setBusy] = useState(false);
	/** Include the current password only for changes to the signed-in account. */
	async function submit(event: SubmitEvent<HTMLFormElement>) {
		event.preventDefault();
		const form = event.currentTarget;
		setBusy(true);
		setError(null);
		setMessage('');
		try {
			await api.updatePassword(user.id, {
				new_password: formValue(form, 'new_password'),
				...(self ? { current_password: formValue(form, 'current_password') } : {}),
			});
			form.reset();
			setMessage('Password updated. Existing sessions have been revoked.');
			onChanged?.();
		}
		catch (err) {
			setError(err);
		}
		finally {
			setBusy(false);
		}
	}
	return (
		<form onSubmit={submit} className="grid gap-4">
			<h2 className="font-semibold">
				{self ? 'Change password' : `Reset password for ${user.username}`}
			</h2>
			{self && (
				<label className="field">
					Current password
					<input
						type="password"
						name="current_password"
						autoComplete="current-password"
						required
					/>
				</label>
			)}
			<label className="field">
				New password
				<input
					type="password"
					name="new_password"
					minLength={12}
					autoComplete="new-password"
					required
				/>
			</label>
			<p className="muted">
				Use at least 12 characters. Changing the password signs this account out of all
				sessions.
			</p>
			<ErrorMessage error={error} />
			{message && <p role="status">{message}</p>}
			<button className="button justify-self-start" disabled={busy}>
				{busy ? 'Saving…' : 'Update password'}
			</button>
		</form>
	);
}

/** Create accounts or edit usernames and roles; reset passwords separately. */
function UserForm({
	api,
	initial,
	onSaved,
	onCancel,
}: {
	api: Api;
	initial: UserDraft;
	onSaved: () => void;
	onCancel: () => void;
}) {
	const [error, setError] = useState<unknown>(null);
	const [busy, setBusy] = useState(false);
	/** Require an initial password only when creating an account. */
	async function submit(event: SubmitEvent<HTMLFormElement>) {
		event.preventDefault();
		setBusy(true);
		setError(null);
		try {
			const form = event.currentTarget;
			const body = { username: formValue(form, 'username'), role: formValue(form, 'role') };
			if (initial.id) await api.updateUser(initial.id, body);
			else await api.createUser({ ...body, password: formValue(form, 'password') });
			onSaved();
		}
		catch (err) {
			setError(err);
		}
		finally {
			setBusy(false);
		}
	}
	return (
		<form className="panel grid gap-4" onSubmit={submit}>
			<h2 className="font-semibold">{initial.id ? 'Edit user' : 'New user'}</h2>
			<div className="grid gap-4 sm:grid-cols-3">
				<label className="field">
					Username
					<input
						name="username"
						defaultValue={initial.username}
						autoComplete="off"
						required
					/>
				</label>
				{!initial.id && (
					<label className="field">
						Password
						<input
							name="password"
							type="password"
							minLength={12}
							autoComplete="new-password"
							required
						/>
					</label>
				)}
				<label className="field">
					Role
					<select name="role" defaultValue={initial.roles?.[0] || initial.role} required>
						<option value="viewer">Viewer</option>
						<option value="editor">Editor</option>
						<option value="admin">Admin</option>
					</select>
				</label>
			</div>
			<ErrorMessage error={error} />
			<div className="flex gap-2">
				<button className="button primary" disabled={busy}>
					{busy ? 'Saving…' : 'Save user'}
				</button>
				<button type="button" className="button" disabled={busy} onClick={onCancel}>
					Cancel
				</button>
			</div>
		</form>
	);
}

/** Manage accounts and notify the app when the current account changes. */
export function Users({
	api,
	currentUser,
	onSelfUpdated,
	onSessionRevoked,
}: {
	api: Api;
	currentUser: SessionUser;
	onSelfUpdated: () => void;
	onSessionRevoked: () => void;
}) {
	const [offset, setOffset] = useState(0);
	const [editing, setEditing] = useState<UserDraft | null>(null);
	const [resetting, setResetting] = useState<User | null>(null);
	const [error, setError] = useState<unknown>(null);
	const [busy, setBusy] = useState(false);
	const load = useCallback(
		(signal: AbortSignal) => api.listUsers({ limit: PAGE_SIZE + 1, offset }, signal),
		[api, offset],
	);
	const list = useList(load, offset, setOffset);
	/** Close forms for deleted accounts and end the session on self-deletion. */
	async function remove(user: User) {
		if (!window.confirm(`Delete ${user.username}? This also revokes their sessions.`)) return;
		setBusy(true);
		setError(null);
		try {
			await api.deleteUser(user.id);
			setEditing((current) => (current?.id === user.id ? null : current));
			setResetting((current) => (current?.id === user.id ? null : current));
			if (user.id === currentUser.id) onSessionRevoked();
			else list.reload();
		}
		catch (err) {
			setError(err);
		}
		finally {
			setBusy(false);
		}
	}
	return (
		<div className="grid gap-5">
			<div className="flex items-center justify-between">
				<h1 className="text-xl font-semibold">Users</h1>
				<button className="button primary" onClick={() => setEditing({ ...emptyUser })}>
					New user
				</button>
			</div>
			{editing && (
				<UserForm
					key={editing.id || 'new'}
					api={api}
					initial={editing}
					onCancel={() => setEditing(null)}
					onSaved={() => {
						setEditing(null);
						list.reload();
						if (editing.id === currentUser.id) onSelfUpdated();
					}}
				/>
			)}
			{resetting && (
				<div className="panel grid gap-4">
					<PasswordForm
						key={resetting.id}
						api={api}
						user={resetting}
						self={resetting.id === currentUser.id}
						onChanged={resetting.id === currentUser.id ? onSessionRevoked : undefined}
					/>
					<button
						className="button justify-self-start"
						onClick={() => setResetting(null)}
					>
						Close
					</button>
				</div>
			)}
			<ErrorMessage error={error || list.error} />
			<div className="overflow-x-auto rounded-lg border border-slate-200 bg-white">
				<table className="w-full">
					<thead className="border-b border-slate-200">
						<tr>
							<th>Username</th>
							<th>Roles</th>
							<th>
								<span className="sr-only">Actions</span>
							</th>
						</tr>
					</thead>
					<tbody className="divide-y divide-slate-100">
						{list.items.map((user) => (
							<tr key={user.id}>
								<td className="font-medium">
									{user.username}
									{user.id === currentUser.id && (
										<span className="ml-2 font-normal muted">(you)</span>
									)}
								</td>
								<td>{user.roles.join(', ') || 'None'}</td>
								<td>
									<div className="flex flex-wrap justify-end gap-3">
										<button
											className="underline"
											onClick={() => setEditing(user)}
										>
											Edit
										</button>
										<button
											className="underline"
											onClick={() => setResetting(user)}
										>
											Password
										</button>
										<button
											className="text-red-700 underline"
											disabled={busy}
											onClick={() => remove(user)}
										>
											Delete
										</button>
									</div>
								</td>
							</tr>
						))}
					</tbody>
				</table>
				{list.loading ? (
					<p role="status" className="p-6 text-center muted">
						Loading users…
					</p>
				) : (
					!list.error &&
					list.items.length === 0 && (
						<p className="p-6 text-center muted">No users found.</p>
					)
				)}
			</div>
			<Pager
				offset={offset}
				setOffset={setOffset}
				hasMore={list.hasMore}
				loading={list.loading}
			/>
		</div>
	);
}
