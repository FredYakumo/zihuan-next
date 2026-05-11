import { clearWorkspaceState } from "./workspace";

export interface WorkspaceControllerOptions {
  createNewTab: () => Promise<void>;
}

export class WorkspaceController {
  constructor(private readonly options: WorkspaceControllerOptions) {}

  async restoreOrCreateTabs(): Promise<void> {
    clearWorkspaceState();
    await this.options.createNewTab();
  }
}
