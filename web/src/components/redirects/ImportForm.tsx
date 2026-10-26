import { useState } from 'react';
import type { SubmitEvent } from 'react';
import type { Api, ImportOutcome } from '../../api';
import { ErrorMessage } from '../shared';

/** Upload a CSV and show the import counts and any row errors. */
export function ImportForm({ api, onImported }: { api: Api; onImported: () => void }) {
	const [result, setResult] = useState<ImportOutcome | null>(null);
	const [error, setError] = useState<unknown>(null);
	const [busy, setBusy] = useState(false);
	/** Submit the selected file and refresh the list when an outcome is returned. */
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
