import createClient from 'openapi-fetch';
import type { components, operations, paths } from './generated/api';

type Schemas = components['schemas'];
export type Redirect = Schemas['RedirectResponse'];
export type User = Schemas['UserResponse'];
export type SessionUser = Schemas['LoginUserResponse'];
export type RedirectDraft = Pick<
	Redirect,
	'slug' | 'destination' | 'redirect_code' | 'is_enabled'
> & { id?: number };
export type UserDraft = Pick<User, 'username'> &
	Partial<Pick<User, 'id' | 'roles'>> &
	Partial<Pick<Schemas['CreateUserRequest'], 'role'>>;
export type ImportOutcome = Schemas['ImportLinksResponse'];

export function errorMessage(error: unknown): string {
	if (error instanceof Error) return error.message;
	if (typeof error === 'string') return error;
	if (
		typeof error === 'object' &&
		error !== null &&
		'message' in error &&
		typeof error.message === 'string'
	) {
		return error.message;
	}
	return 'An unexpected error occurred.';
}

export class ApiError extends Error {
	constructor(
		public readonly status: number,
		body: unknown,
	) {
		super(body ? errorMessage(body) : `Request failed (${status})`);
		this.name = 'ApiError';
	}
}

function unwrap<T>({
	data,
	error,
	response,
}: {
	data?: T;
	error?: unknown;
	response: Response;
}): T {
	if (!response.ok) throw new ApiError(response.status, error);
	if (data === undefined) throw new Error('The API returned an empty response.');
	return data;
}

export function createApi(token = '', onUnauthorized?: () => void) {
	const client = createClient<paths>({
		headers: token ? { Authorization: `Bearer ${token}` } : {},
		// Some Axum validation failures are plain text rather than JSON.
		// Normalize only those failures so openapi-fetch can preserve their messages.
		fetch: async (input) => {
			const response = await fetch(input);
			if (
				!response.ok &&
				!response.headers.get('content-type')?.includes('application/json')
			) {
				const headers = new Headers(response.headers);
				headers.set('content-type', 'application/json');
				return new Response(JSON.stringify({ message: await response.text() }), {
					status: response.status,
					headers,
				});
			}
			return response;
		},
	});
	client.use({
		onResponse({ response }) {
			if (response.status === 401) onUnauthorized?.();
		},
	});
	return {
		async login(body: Schemas['LoginRequest']) {
			const data = unwrap(await client.POST('/api/v1/auth/login', { body }));
			if (!data.token || !data.user) {
				throw new Error('The API did not return a login session.');
			}
			return { token: data.token, user: data.user };
		},
		async me(signal?: AbortSignal) {
			const data = unwrap(await client.GET('/api/v1/auth/me', { signal }));
			if (!data.user) throw new Error('The API did not return your account.');
			return data.user;
		},
		async logout() {
			return unwrap(await client.POST('/api/v1/auth/logout'));
		},
		async listRedirects(
			query: operations['list_redirects']['parameters']['query'],
			signal: AbortSignal,
		) {
			return unwrap(await client.GET('/api/v1/redirects', { params: { query }, signal }))
				.redirects;
		},
		async createRedirect(body: Schemas['CreateRedirectRequest']) {
			return unwrap(await client.POST('/api/v1/redirects', { body }));
		},
		async updateRedirect(id: number, body: Schemas['UpdateRedirectRequest']) {
			return unwrap(
				await client.PATCH('/api/v1/redirects/{id}', { params: { path: { id } }, body }),
			);
		},
		async deleteRedirect(id: number) {
			return unwrap(
				await client.DELETE('/api/v1/redirects/{id}', { params: { path: { id } } }),
			);
		},
		async importRedirects(file: File): Promise<ImportOutcome> {
			const result = await client.POST('/api/v1/redirects/import', {
				body: { file },
				bodySerializer(body) {
					const form = new FormData();
					form.append('file', body.file);
					return form;
				},
			});
			// A 422 import is a structured outcome with row errors, not a generic failure.
			if (
				result.response.status === 422 &&
				typeof result.error === 'object' &&
				'errors' in result.error
			) {
				return result.error;
			}
			return unwrap(result);
		},
		async listUsers(
			query: operations['list_users']['parameters']['query'],
			signal: AbortSignal,
		) {
			return unwrap(await client.GET('/api/v1/users', { params: { query }, signal })).users;
		},
		async createUser(body: Schemas['CreateUserRequest']) {
			return unwrap(await client.POST('/api/v1/users', { body }));
		},
		async updateUser(id: number, body: Schemas['UpdateUserRequest']) {
			return unwrap(
				await client.PATCH('/api/v1/users/{id}', { params: { path: { id } }, body }),
			);
		},
		async deleteUser(id: number) {
			return unwrap(await client.DELETE('/api/v1/users/{id}', { params: { path: { id } } }));
		},
		async updatePassword(id: number, body: Schemas['UpdatePasswordRequest']) {
			return unwrap(
				await client.PUT('/api/v1/users/{id}/password', { params: { path: { id } }, body }),
			);
		},
	};
}

export type Api = ReturnType<typeof createApi>;
