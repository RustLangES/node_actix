export function actix(): ActixApp;
export default actix;

export class ActixApp {
  hostname?: string;
  port?: number;

  get(path: string, callback: (req: Request) => void): void;

  listen(port: number, callback?: (server: ActixApp) => void): Promise<void>;
  listen(
    port: number,
    hostname?: string,
    callback?: (server: ActixApp) => void,
  ): Promise<void>;
}
