import { toast } from 'svelte-sonner';
import { api } from '#lib/api-client.ts';
import { toastError } from '#lib/toast.ts';
import { confirm, type ConfirmRequest } from '#lib/confirm.svelte.ts';
import { formatNumber } from '#lib/format.ts';
import { adminUserPatchSchema, type AdminUserPatch } from '#lib/schemas.ts';
import type { User } from '#lib/types.ts';

export interface UserAction {
	label: string;
	changes: AdminUserPatch;
	/** Success toast. */
	done: { title: string; description?: string };
	ask?: ConfirmRequest;
}

/** The changes an admin can make to `u`, in menu order; role and disable aren't offered on yourself. */
export function userActions(u: User, self: boolean): UserAction[] {
	const admin = u.role === 'admin';
	return [
		...(self
			? []
			: [
					{
						label: admin ? 'Remove admin' : 'Make admin',
						changes: { role: admin ? 'user' : 'admin' },
						done: admin
							? { title: `${u.email} is no longer an admin` }
							: {
									title: `${u.email} is now an admin`,
									description: 'They can open the admin pages.'
								},
						ask: admin
							? {
									title: `Remove admin from ${u.email}?`,
									description:
										'They lose access to the admin pages: users, every link, reports and retired slugs.',
									action: 'Remove admin',
									cancel: 'Keep admin'
								}
							: {
									title: `Make ${u.email} an admin?`,
									description:
										'They can see and change every user and link, resolve reports, and promote or disable other admins.',
									action: 'Make admin',
									cancel: 'Cancel'
								}
					} as const,
					u.disabled
						? {
								label: 'Enable',
								changes: { disabled: false },
								done: {
									title: `${u.email} enabled`,
									description: 'They can sign in again, and their links redirect.'
								}
							}
						: {
								label: 'Disable',
								changes: { disabled: true },
								done: {
									title: `${u.email} disabled`,
									description: 'They’re signed out, and their links stop redirecting.'
								},
								ask: {
									title: `Disable ${u.email}?`,
									description:
										'They are signed out, can’t sign in, and none of their links redirect until you enable them again.',
									action: 'Disable user',
									cancel: 'Keep enabled',
									destructive: true
								}
							}
				]),
		u.email_verified
			? {
					label: 'Mark unverified',
					changes: { email_verified: false },
					done: { title: `${u.email} marked unverified` }
				}
			: {
					label: 'Mark verified',
					changes: { email_verified: true },
					done: { title: `${u.email} marked verified` }
				}
	];
}

/** Runs `a` on `u` (confirming first if it asks); toasts the outcome. Returns the updated user, or null. */
export async function applyUserAction(u: User, a: UserAction): Promise<User | null> {
	if (a.ask && !(await confirm(a.ask))) return null;
	try {
		const updated = await api<User>(
			'PATCH',
			`/admin/users/${u.id}`,
			adminUserPatchSchema.parse(a.changes)
		);
		toast.success(a.done.title, { description: a.done.description });
		return updated;
	} catch (e) {
		toastError(`Couldn't update ${u.email}`, e);
		return null;
	}
}

/** Confirms, then deletes `u`; true once deleted. */
export async function deleteUser(u: User & { link_count: number }): Promise<boolean> {
	const ok = await confirm({
		title: `Delete ${u.email}?`,
		description: `Their ${formatNumber(u.link_count)} links and all stats are deleted, and permanent or blocked slugs are retired until an admin releases them. This can't be undone.`,
		action: 'Delete user',
		cancel: 'Keep user',
		destructive: true
	});
	if (!ok) return false;
	try {
		await api('DELETE', `/admin/users/${u.id}`);
		toast.success(`${u.email} deleted`, {
			description:
				u.link_count === 1
					? 'Their link was deleted too.'
					: `Their ${formatNumber(u.link_count)} links were deleted too.`
		});
		return true;
	} catch (e) {
		toastError(`Couldn't delete ${u.email}`, e);
		return false;
	}
}
