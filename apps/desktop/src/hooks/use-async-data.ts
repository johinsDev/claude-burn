import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";

/**
 * Guarda siempre el callback mas reciente sin volverlo una dependencia.
 *
 * La asignacion va en un layout effect y no durante el render: mutar una ref
 * mientras se renderiza rompe con el render concurrente, que puede descartar
 * un render a medias y dejar la ref apuntando a algo que nunca se monto.
 */
export function useLatest<T>(value: T) {
  const ref = useRef(value);
  useLayoutEffect(() => {
    ref.current = value;
  });
  return ref;
}

export type AsyncData<T> = {
  data: T | null;
  loading: boolean;
  error: string | null;
  /** Vuelve a pedir los datos; util tras un sync manual. */
  reload: () => void;
};

/**
 * Carga datos del backend con cancelacion.
 *
 * El contador de generacion importa: al cambiar de sesion en el drill-down se
 * disparan dos consultas y la primera puede volver despues. Sin el contador,
 * una respuesta vieja pisa a la nueva y el grafico muestra otra sesion.
 */
export function useAsyncData<T>(fn: () => Promise<T>, deps: unknown[]): AsyncData<T> {
  const [data, setData] = useState<T | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [nonce, setNonce] = useState(0);
  const generation = useRef(0);

  const fnRef = useLatest(fn);

  useEffect(() => {
    const mine = ++generation.current;
    let alive = true;

    fnRef
      .current()
      .then((value) => {
        // Descarta lo que ya no es la peticion vigente.
        if (!alive || mine !== generation.current) return;
        setData(value);
        setError(null);
        setLoading(false);
      })
      .catch((e: unknown) => {
        if (!alive || mine !== generation.current) return;
        setError(e instanceof Error ? e.message : String(e));
        setLoading(false);
      });

    return () => {
      alive = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [...deps, nonce]);

  const reload = useCallback(() => setNonce((n) => n + 1), []);
  return { data, loading, error, reload };
}

/** Dispara `fn` cada `ms` mientras el componente este montado. */
export function useInterval(fn: () => void, ms: number) {
  const ref = useLatest(fn);
  useEffect(() => {
    const id = setInterval(() => ref.current(), ms);
    return () => clearInterval(id);
  }, [ms, ref]);
}
