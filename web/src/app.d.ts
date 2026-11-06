declare global {
	const __REPO_URL__: string;
	const __APP_VERSION__: string;
	namespace App {
		namespace Superforms {
			type Message = string;
		}
	}
}

export {};
