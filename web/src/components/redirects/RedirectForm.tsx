import { useState } from 'react';
import type { ChangeEvent, SubmitEvent } from 'react';
import type { Api, RedirectDraft } from '../../api';
import { ErrorMessage } from '../shared';

/** Share the same draft form for creating and editing redirects. */
export function RedirectForm({
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
	/** Convert checkbox and status-code inputs to the types the API expects. */
	function change(event: ChangeEvent<HTMLInputElement | HTMLSelectElement>) {
		const { name, value } = event.target;
		const checked = event.target instanceof HTMLInputElement && event.target.checked;
		setDraft({
			...draft,
			[name]:
				name === 'is_enabled' ? checked : name === 'redirect_code' ? Number(value) : value,
		});
	}
	/** Create or update the redirect, keeping the draft if saving fails. */
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
