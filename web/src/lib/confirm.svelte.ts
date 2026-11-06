export interface ConfirmRequest {
	title: string;
	description: string;
	action: string;
	cancel?: string;
	destructive?: boolean;
	/** Text the user must type to enable the action. */
	typeToConfirm?: string;
}

export const pending = $state<{
	req: (ConfirmRequest & { resolve: (ok: boolean) => void }) | null;
}>({ req: null });

/** Promise-based replacement for window.confirm, rendered by ConfirmDialog in the app layout. */
export const confirm = (req: ConfirmRequest) =>
	new Promise<boolean>((resolve) => {
		pending.req?.resolve(false);
		pending.req = { ...req, resolve };
	});
