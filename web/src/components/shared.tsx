import { useEffect, useState } from 'react';
import { errorMessage } from '../api';
export const PAGE_SIZE = 20;

export function ErrorMessage({ error }: { error: unknown }) {
	return error ? (
		<p role="alert" className="rounded-md border border-red-200 bg-red-50 p-3 text-red-800">
			{errorMessage(error)}
		</p>
	) : null;
}

export function useList<T>(
	load: (signal: AbortSignal) => Promise<T[]>,
	offset = 0,
	setOffset?: (offset: number) => void,
) {
	const [state, setState] = useState<{
		items: T[];
		hasMore: boolean;
		loading: boolean;
		error: unknown;
	}>({
		items: [],
		hasMore: false,
		loading: true,
		error: null,
	});
	const [version, setVersion] = useState(0);
	useEffect(() => {
		const controller = new AbortController();
		setState({ items: [], hasMore: false, loading: true, error: null });
		load(controller.signal)
			.then((items) => {
				if (controller.signal.aborted) return;
				if (items.length === 0 && offset > 0 && setOffset) {
					setOffset(Math.max(0, offset - PAGE_SIZE));
					return;
				}
				setState({
					items: items.slice(0, PAGE_SIZE),
					hasMore: items.length > PAGE_SIZE,
					loading: false,
					error: null,
				});
			})
			.catch((err) => {
				if (!controller.signal.aborted) {
					setState({ items: [], hasMore: false, loading: false, error: err });
				}
			});
		return () => controller.abort();
	}, [load, offset, setOffset, version]);
	return { ...state, reload: () => setVersion((value) => value + 1) };
}

export function Pager({
	offset,
	setOffset,
	hasMore,
	loading,
}: {
	offset: number;
	setOffset: (offset: number) => void;
	hasMore: boolean;
	loading: boolean;
}) {
	return (
		<div className="mt-4 flex items-center justify-between gap-3">
			<p className="muted">Page {offset / PAGE_SIZE + 1}</p>
			<div className="flex gap-2">
				<button
					className="button"
					disabled={loading || offset === 0}
					onClick={() => setOffset(Math.max(0, offset - PAGE_SIZE))}
				>
					Previous
				</button>
				<button
					className="button"
					disabled={loading || !hasMore}
					onClick={() => setOffset(offset + PAGE_SIZE)}
				>
					Next
				</button>
			</div>
		</div>
	);
}

export function formValue(form: HTMLFormElement, name: string): string {
	const value = new FormData(form).get(name);
	if (typeof value !== 'string') throw new Error(`Missing form field: ${name}`);
	return value;
}
