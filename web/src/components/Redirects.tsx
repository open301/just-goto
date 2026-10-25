import { useCallback, useEffect, useRef, useState } from 'react';
import type { ChangeEvent, SubmitEvent } from 'react';
import type { Api, Redirect, RedirectDraft, ImportOutcome } from '../api';
import { ErrorMessage, Pager, PAGE_SIZE, useList } from './shared';

const emptyRedirect: RedirectDraft = {
	slug: '',
	destination: '',
	redirect_code: 302,
	is_enabled: true,
};

function RedirectForm({
	initial,
	api,
	onSaved,
	onCancel,
}: {
	initial: RedirectDraft;
	api: Api;
	onSaved: () => void;
	onCancel: () => void;
}) {
	const [draft, setDraft] = useState(initial);
	const [error, setError] = useState<unknown>(null);
	const [busy, setBusy] = useState(false);
	function change(event: ChangeEvent<HTMLInputElement | HTMLSelectElement>) {
		const { name, value } = event.target;
		const checked = event.target instanceof HTMLInputElement && event.target.checked;
		setDraft({
			...draft,
			[name]:
				name === 'is_enabled' ? checked : name === 'redirect_code' ? Number(value) : value,
		});
	}
	async function save(event: SubmitEvent<HTMLFormElement>) {
		event.preventDefault();
		setBusy(true);
		setError(null);
		try {
			const { slug, destination, redirect_code, is_enabled } = draft;
			const body = { slug, destination, redirect_code, is_enabled };
			if (initial.id) await api.updateRedirect(initial.id, body);
			else await api.createRedirect(body);
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
		<form className="panel grid gap-4" onSubmit={save}>
			<h2 className="text-base font-semibold">
				{initial.id ? 'Edit redirect' : 'New redirect'}
			</h2>
			<div className="grid gap-4 sm:grid-cols-2">
				<label className="field">
					Slug
					<input
						name="slug"
						value={draft.slug}
						onChange={change}
						pattern={'[A-Za-z0-9_\\-]+'}
						maxLength={128}
						placeholder="my-link"
						required
						autoFocus
					/>
				</label>
				<label className="field">
					Destination
					<input
						name="destination"
						type="url"
						value={draft.destination}
						onChange={change}
						maxLength={2048}
						placeholder="https://example.com"
						required
					/>
				</label>
				<label className="field">
					Redirect code
					<select name="redirect_code" value={draft.redirect_code} onChange={change}>
						<option value={301}>301 - Permanent</option>
						<option value={302}>302 - Temporary</option>
						<option value={307}>307 - Temporary, preserve method</option>
						<option value={308}>308 - Permanent, preserve method</option>
					</select>
				</label>
				<label className="flex items-center gap-2">
					<input
						name="is_enabled"
						type="checkbox"
						checked={draft.is_enabled}
						onChange={change}
					/>
					Enabled
				</label>
			</div>
			<ErrorMessage error={error} />
			<div className="flex gap-2">
				<button className="button primary" disabled={busy}>
					{busy ? 'Saving...' : 'Save redirect'}
				</button>
				<button type="button" className="button" disabled={busy} onClick={onCancel}>
					Cancel
				</button>
			</div>
		</form>
	);
}

function ImportForm({ api, onImported }: { api: Api; onImported: () => void }) {
	const [result, setResult] = useState<ImportOutcome | null>(null);
	const [error, setError] = useState<unknown>(null);
	const [busy, setBusy] = useState(false);
	async function submit(event: SubmitEvent<HTMLFormElement>) {
		event.preventDefault();
		const form = event.currentTarget;
		setBusy(true);
		setError(null);
		setResult(null);
		try {
			const file = new FormData(form).get('file');
			if (!(file instanceof File)) throw new Error('Choose a CSV file.');
			const data = await api.importRedirects(file);
			setResult(data);
			form.reset();
			onImported();
		}
		catch (err) {
			setError(err);
		}
		finally {
			setBusy(false);
		}
	}
	return (
		<details className="panel">
			<summary className="cursor-pointer font-medium">Import CSV</summary>
			<p className="mt-3 muted">
				Required columns: slug, destination. Optional: redirect_code, is_enabled. Up to 10
				MiB and 10,000 rows.
			</p>
			<form onSubmit={submit} className="mt-3 flex flex-wrap items-end gap-3">
				<label className="field flex-1">
					CSV file
					<input type="file" name="file" accept=".csv,text/csv" required />
				</label>
				<button className="button" disabled={busy}>
					{busy ? 'Importing…' : 'Import'}
				</button>
			</form>
			<div className="mt-3">
				<ErrorMessage error={error} />
			</div>
			{result && (
				<div role="status" className="mt-3">
					<p>
						{result.imported} imported, {result.skipped} skipped.
					</p>
					{result.errors.length > 0 && (
						<ul className="mt-2 max-h-48 list-inside list-disc overflow-auto text-red-800">
							{result.errors.map((rowError, index) => (
								<li key={index}>
									Row {rowError.row}
									{rowError.slug ? ` (${rowError.slug})` : ''}: {rowError.message}
								</li>
							))}
						</ul>
					)}
				</div>
			)}
		</details>
	);
}

export function Redirects({ api, canEdit }: { api: Api; canEdit: boolean }) {
	const [slugInput, setSlugInput] = useState('');
	const [filter, setFilter] = useState({ slug: '', enabled: '' });
	const [offset, setOffset] = useState(0);
	const [editing, setEditing] = useState<RedirectDraft | null>(null);
	const [copiedId, setCopiedId] = useState<number | null>(null);
	const [error, setError] = useState<unknown>(null);
	const [busy, setBusy] = useState(false);
	const copyTimer = useRef<number | null>(null);
	useEffect(
		() => () => {
			if (copyTimer.current !== null) window.clearTimeout(copyTimer.current);
		},
		[],
	);
	const load = useCallback(
		(signal: AbortSignal) =>
			api.listRedirects(
				{
					limit: PAGE_SIZE + 1,
					offset,
					slug: filter.slug,
					...(filter.enabled ? { enabled: filter.enabled === 'true' } : {}),
				},
				signal,
			),
		[api, offset, filter],
	);
	const list = useList(load, offset, setOffset);
	async function copyShortUrl(redirect: Redirect) {
		setError(null);
		try {
			const url = new URL(
				`/${encodeURIComponent(redirect.slug)}`,
				window.location.origin,
			).toString();
			if (!navigator.clipboard?.writeText) {
				throw new Error('Clipboard access is not available.');
			}
			await navigator.clipboard.writeText(url);
			setCopiedId(redirect.id);
			if (copyTimer.current !== null) window.clearTimeout(copyTimer.current);
			copyTimer.current = window.setTimeout(() => {
				setCopiedId((current) => (current === redirect.id ? null : current));
				copyTimer.current = null;
			}, 1800);
		}
		catch (err) {
			setError(err);
		}
	}
	async function toggleEnabled(redirect: Redirect) {
		setBusy(true);
		setError(null);
		try {
			const nextEnabled = !redirect.is_enabled;
			await api.updateRedirect(redirect.id, { is_enabled: nextEnabled });
			setEditing((current) =>
				current?.id === redirect.id ? { ...current, is_enabled: nextEnabled } : current,
			);
			list.reload();
		}
		catch (err) {
			setError(err);
		}
		finally {
			setBusy(false);
		}
	}
	async function remove(redirect: Redirect) {
		if (!window.confirm(`Delete /${redirect.slug}?`)) return;
		setBusy(true);
		setError(null);
		try {
			await api.deleteRedirect(redirect.id);
			setEditing((current) => (current?.id === redirect.id ? null : current));
			list.reload();
		}
		catch (err) {
			setError(err);
		}
		finally {
			setBusy(false);
		}
	}
	function syncFilterAndReload() {
		const slug = slugInput.trim();
		setSlugInput(slug);
		if (slug !== filter.slug) {
			setFilter({ ...filter, slug });
			setOffset(0);
		}
		else {
			list.reload();
		}
	}
	return (
		<div className="grid gap-5">
			<div className="flex items-center justify-between gap-3">
				<div>
					<h1 className="text-xl font-semibold">Redirects</h1>
					<p className="mt-1 muted">Your short links and where they go.</p>
				</div>
				{canEdit && (
					<button
						className="button primary"
						onClick={() => setEditing({ ...emptyRedirect })}
					>
						New redirect
					</button>
				)}
			</div>
			{editing && (
				<RedirectForm
					key={`${editing.id || 'new'}-${editing.is_enabled}`}
					initial={editing}
					api={api}
					onCancel={() => setEditing(null)}
					onSaved={() => {
						setEditing(null);
						list.reload();
					}}
				/>
			)}
			<form
				className="flex flex-wrap gap-2"
				onSubmit={(event) => {
					event.preventDefault();
					const slug = slugInput.trim();
					setSlugInput(slug);
					if (slug !== filter.slug || offset !== 0) {
						setFilter({ ...filter, slug });
						setOffset(0);
					}
					else {
						list.reload();
					}
				}}
			>
				<label className="min-w-40 flex-1">
					<span className="sr-only">Search slugs</span>
					<input
						name="slug"
						value={slugInput}
						onChange={(event) => setSlugInput(event.target.value)}
						placeholder="Search slugs…"
					/>
				</label>
				<label>
					<span className="sr-only">Status</span>
					<select
						name="enabled"
						value={filter.enabled}
						onChange={(event) => {
							const slug = slugInput.trim();
							setSlugInput(slug);
							setFilter({ slug, enabled: event.target.value });
							setOffset(0);
						}}
					>
						<option value="">All statuses</option>
						<option value="true">Enabled</option>
						<option value="false">Disabled</option>
					</select>
				</label>
				<button className="button">Search</button>
				<button
					type="button"
					className="button"
					onClick={syncFilterAndReload}
					disabled={list.loading}
				>
					Refresh
				</button>
			</form>
			<ErrorMessage error={error || list.error} />
			<div className="overflow-x-auto rounded-lg border border-slate-200 bg-white">
				<table className="w-full">
					<thead className="border-b border-slate-200">
						<tr>
							<th>Short link / Destination</th>
							<th>Code</th>
							<th>Status</th>
							<th>
								<span className="sr-only">Actions</span>
							</th>
						</tr>
					</thead>
					<tbody className="divide-y divide-slate-100">
						{list.items.map((redirect) => (
							<tr key={redirect.id}>
								<td>
									<a
										className="inline-block max-w-[40vw] truncate align-top font-medium underline sm:max-w-md"
										href={`/${encodeURIComponent(redirect.slug)}`}
										target="_blank"
										rel="noreferrer"
										title={`/${redirect.slug}`}
									>
										/{redirect.slug}
									</a>
									<p
										className="mt-1 max-w-[40vw] truncate muted sm:max-w-md"
										title={redirect.destination}
									>
										{redirect.destination}
									</p>
								</td>
								<td>{redirect.redirect_code}</td>
								<td>
									<span
										className={
											redirect.is_enabled ? 'text-emerald-700' : 'muted'
										}
									>
										{redirect.is_enabled ? 'Enabled' : 'Disabled'}
									</span>
								</td>
								<td>
									<div className="flex flex-wrap justify-end gap-3">
										<button
											type="button"
											className="underline"
											onClick={() => copyShortUrl(redirect)}
										>
											{copiedId === redirect.id ? 'Copied' : 'Copy'}
										</button>
										{canEdit && (
											<>
												<button
													type="button"
													className="underline"
													disabled={busy}
													onClick={() => toggleEnabled(redirect)}
												>
													{redirect.is_enabled ? 'Disable' : 'Enable'}
												</button>
												<button
													type="button"
													className="underline"
													onClick={() => setEditing(redirect)}
												>
													Edit
												</button>
												<button
													type="button"
													className="text-red-700 underline"
													disabled={busy}
													onClick={() => remove(redirect)}
												>
													Delete
												</button>
											</>
										)}
									</div>
								</td>
							</tr>
						))}
					</tbody>
				</table>
				{list.loading ? (
					<p role="status" className="p-6 text-center muted">
						Loading redirects…
					</p>
				) : (
					!list.error &&
					list.items.length === 0 && (
						<p className="p-6 text-center muted">No redirects found.</p>
					)
				)}
			</div>
			<Pager
				offset={offset}
				setOffset={setOffset}
				hasMore={list.hasMore}
				loading={list.loading}
			/>
			{canEdit && <ImportForm api={api} onImported={list.reload} />}
		</div>
	);
}
