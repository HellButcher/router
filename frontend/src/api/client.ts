import createClient from "openapi-fetch";
import type { paths } from "./types.js";

export const client = createClient<paths>({ baseUrl: "/" });
