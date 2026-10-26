import type { Redirect } from '../../api';

type RedirectActions = {
	canEdit: boolean;
	busy: boolean;
	copiedId: number | null;
	onCopy: (redirect: Redirect) => void;
	onToggle: (redirect: Redirect) => void;
	onEdit: (redirect: Redirect) => void;
	onDelete: (redirect: Redirect) => void;
};

/** Show one redirect and the actions allowed for the current account. */
function RedirectRow({
	redirect,
	canEdit,
	busy,
	copiedId,
	onCopy,
	onToggle,
	onEdit,
	onDelete,
}: { redirect: Redirect } & RedirectActions) {
	return (
		<tr>
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
				<span className={redirect.is_enabled ? 'text-emerald-700' : 'muted'}>
					{redirect.is_enabled ? 'Enabled' : 'Disabled'}
				</span>
			</td>
			<td>
				<div className="flex flex-wrap justify-end gap-3">
					<button type="button" className="underline" onClick={() => onCopy(redirect)}>
						{copiedId === redirect.id ? 'Copied' : 'Copy'}
					</button>
					{canEdit && (
						<>
							<button
								type="button"
								className="underline"
								disabled={busy}
								onClick={() => onToggle(redirect)}
							>
								{redirect.is_enabled ? 'Disable' : 'Enable'}
							</button>
							<button
								type="button"
								className="underline"
								onClick={() => onEdit(redirect)}
							>
								Edit
							</button>
							<button
								type="button"
								className="text-red-700 underline"
								disabled={busy}
								onClick={() => onDelete(redirect)}
							>
								Delete
							</button>
						</>
					)}
				</div>
			</td>
		</tr>
	);
}

/** Display redirect rows with loading and empty states. */
export function RedirectTable({
	redirects,
	loading,
	error,
	...actions
}: {
	redirects: Redirect[];
	loading: boolean;
	error: unknown;
} & RedirectActions) {
	return (
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
					{redirects.map((redirect) => (
						<RedirectRow key={redirect.id} redirect={redirect} {...actions} />
					))}
				</tbody>
			</table>
			{loading && (
				<p role="status" className="p-6 text-center muted">
					Loading redirects…
				</p>
			)}
			{!loading && !error && redirects.length === 0 && (
				<p className="p-6 text-center muted">No redirects found.</p>
			)}
		</div>
	);
}
