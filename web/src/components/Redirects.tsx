import { useCallback, useEffect, useRef, useState } from 'react';
import type { Api, Redirect, RedirectDraft } from '../api';
import { ErrorMessage, Pager, PAGE_SIZE, useList } from './shared';
import { RedirectForm } from './redirects/RedirectForm';
import { ImportForm } from './redirects/ImportForm';
import { RedirectFilters } from './redirects/RedirectFilters';
import { RedirectTable } from './redirects/RedirectTable';

const emptyRedirect: RedirectDraft = {
	slug: '',
	destination: '',
	redirect_code: 302,
	is_enabled: true,
};

/** Browse filtered redirects and expose editing tools to admins and editors. */
export function Redirects({ api, canEdit }: { api: Api; canEdit: boolean }) {
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
	/** Copy the public URL and briefly mark the matching row as copied. */
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
	/** Toggle a redirect and keep any open edit form in sync. */
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
	/** Confirm deletion, close a matching edit form, and refresh the list. */
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
	/** Apply search filters from the first page, or reload if unchanged. */
	function search(slug: string, enabled: string) {
		if (slug !== filter.slug || enabled !== filter.enabled || offset !== 0) {
			setFilter({ slug, enabled });
			setOffset(0);
		}
		else {
			list.reload();
		}
	}
	/** Apply pending search text, preserving the page if the filter is unchanged. */
	function refresh(slug: string) {
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
			<RedirectFilters
				enabled={filter.enabled}
				loading={list.loading}
				onSearch={search}
				onRefresh={refresh}
			/>
			<ErrorMessage error={error || list.error} />
			<RedirectTable
				redirects={list.items}
				loading={list.loading}
				error={list.error}
				canEdit={canEdit}
				busy={busy}
				copiedId={copiedId}
				onCopy={copyShortUrl}
				onToggle={toggleEnabled}
				onEdit={setEditing}
				onDelete={remove}
			/>
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
