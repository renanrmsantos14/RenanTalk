// Peças pequenas repetidas nas abas.
import { useEffect, useRef } from "react";

export const Salvo = ({ on }: { on: boolean }) => <span className={"saved" + (on ? " on" : "")}>✓ Salvo</span>;

export const Msg = ({ texto, erro, mut, estilo }: { texto: string; erro?: boolean; mut?: boolean; estilo?: React.CSSProperties }) => (
  <div className={"msg" + (erro ? " err" : mut ? " mut" : texto ? " ok" : "")} style={estilo}>{texto}</div>
);

/** Slider que grava só ao soltar (o evento nativo `change`); `onChange` do React dispara a cada movimento. */
export function Slider({ valor, min, max, passo, rotulo, desligado, aoMover, aoSoltar }: {
  valor: number; min: number; max: number; passo: number; rotulo: string; desligado?: boolean;
  aoMover: (v: number) => void; aoSoltar: (v: number) => void;
}) {
  const ref = useRef<HTMLInputElement>(null);
  const solto = useRef(aoSoltar);
  solto.current = aoSoltar;
  useEffect(() => {
    const el = ref.current!, f = () => solto.current(+el.value);
    el.addEventListener("change", f);
    return () => el.removeEventListener("change", f);
  }, []);
  return <input ref={ref} type="range" min={min} max={max} step={passo} value={valor} disabled={desligado} aria-label={rotulo} onChange={(e) => aoMover(+e.target.value)} />;
}
