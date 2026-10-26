import { useState } from 'react';
import type { SubmitEvent } from 'react';

/** Keep search text local until a search, status change, or refresh applies it. */
export function RedirectFilters({
	enabled,
	loading,
	onSearch,
	onRefresh,
}: {
	enabled: string;
	loading: boolean;
	onSearch: (slug: string, enabled: string) => void;
	onRefresh: (slug: string) => void;
}) {
	const [slugInput, setSlugInput] = useState('');
	/** Trim the draft so displayed search text matches the applied filter. */
	function readSlug() {
		const slug = slugInput.trim();
		setSlugInput(slug);
		return slug;
	}
	/** Apply the current text and status when the search form is submitted. */
	function submit(event: SubmitEvent<HTMLFormElement>) {
		event.preventDefault();
		onSearch(readSlug(), enabled);
	}
	return (
		<form className="flex flex-wrap gap-2" onSubmit={submit}>
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
					value={enabled}
					onChange={(event) => onSearch(readSlug(), event.target.value)}
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
				onClick={() => onRefresh(readSlug())}
				disabled={loading}
			>
				Refresh
			</button>
		</form>
	);
}
