import { createClient, type Session, type User } from "@supabase/supabase-js";

/** Built-in TuffSwarm Supabase project (must match Rust `BUILTIN_SUPABASE_*`). */
export const TUFFSWARM_SUPABASE_URL = "https://vsoqnwknpueuubiovyjd.supabase.co";
export const TUFFSWARM_SUPABASE_ANON_KEY =
  "sb_publishable_b0ICBMz_HvyRa8GioadWcg_Co5Vjljr";

export const supabase = createClient(
  TUFFSWARM_SUPABASE_URL,
  TUFFSWARM_SUPABASE_ANON_KEY,
  {
    auth: {
      persistSession: true,
      autoRefreshToken: true,
      detectSessionInUrl: false,
      storageKey: "tuffbox-crash-votes-auth",
    },
  },
);

export type AuthSnapshot = {
  session: Session | null;
  user: User | null;
};

export async function getAuthSnapshot(): Promise<AuthSnapshot> {
  const { data, error } = await supabase.auth.getSession();
  if (error) throw error;
  return { session: data.session, user: data.session?.user ?? null };
}

export async function signUpWithEmail(email: string, password: string) {
  const { data, error } = await supabase.auth.signUp({
    email: email.trim(),
    password,
  });
  if (error) throw error;
  return data;
}

export async function signInWithEmail(email: string, password: string) {
  const { data, error } = await supabase.auth.signInWithPassword({
    email: email.trim(),
    password,
  });
  if (error) throw error;
  return data;
}

export async function signOut() {
  const { error } = await supabase.auth.signOut();
  if (error) throw error;
}
