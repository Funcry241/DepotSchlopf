"""Vorab-Rückschau: Stärke-Regel auf DAX-Schlusskursen 2000–2014, Kontogröße und Gebührenmodell im Vergleich.
Aufruf: python3 -I rueckschau.py <quotes.txt>
Regeln: Signal aus Schluss t-1 (Rendite über R Tage), Ausführung zum Schluss t mit 5 bp Spread je Seite,
ganze Stücke, höchstens N Werte zu je Anteil A, Prüfung alle H Handelstage, Tausch nur wenn ein Wert die Top-N verlässt.
"""
import sys, math
import pandas as pd, numpy as np
def lade(p):
    z=[]
    for l in open(p,encoding='latin-1'):
        t=[s.strip() for s in l.strip().split(';')]
        if len(t)<5 or not t[3] or not t[4]: continue
        f=lambda s: float(s.replace('.','').replace(',','.'))
        z.append((t[1],pd.to_datetime(t[2],format='%d.%m.%y'),f(t[4])))
    df=pd.DataFrame(z,columns=['n','d','c'])
    c=df.pivot_table(index='d',columns='n',values='c')
    return c.loc[c['DAX'].dropna().index]
def gebuehr(v,modell):
    prov=max(0.0005*v,1.25)
    if modell=='papier': return prov+0.25
    if modell=='B': return prov+max(0.00004*v,0.50)+0.02+0.000008*v+0.01
    if modell=='C': return prov+max(0.000072*v,0.90)+0.02+0.000008*v+0.01
    if modell=='fix': return max(0.0005*v,3.0)
    if modell=='null': return 0.0
def lauf(c,start,N,A,H,R,modell,spread=0.0005,minorder=100.0,von=None,bis=None):
    werte=[n for n in c.columns if n!='DAX']
    px=c[werte]
    idx=c.index
    i0=max(R+1, 0 if von is None else idx.searchsorted(von))
    i1=len(idx) if bis is None else idx.searchsorted(bis)
    bar=start; pos={}; geb=0.0; orders=0; kurve=[]
    for i in range(i0,i1):
        heute=px.iloc[i]
        if (i-i0)%H==0:
            sig=(px.iloc[i-1]/px.iloc[i-1-R]-1).dropna()
            sig=sig[[n for n in sig.index if not math.isnan(heute[n])]]
            rang=list(sig.sort_values(ascending=False).index)
            wert=bar+sum(s*heute[n] for n,s in pos.items() if not math.isnan(heute[n]))
            ziel_betrag=A*wert
            # Top-N unter den bezahlbaren Werten
            top=[n for n in rang if heute[n]*(1+spread)<=ziel_betrag and int(ziel_betrag//(heute[n]*(1+spread)))*heute[n]>=minorder][:N]
            for n in list(pos):
                if n not in top and not math.isnan(heute[n]):
                    v=pos[n]*heute[n]*(1-spread); g=gebuehr(v,modell)
                    bar+=v-g; geb+=g; orders+=1; del pos[n]
            for n in top:
                if n in pos: continue
                p=heute[n]*(1+spread)
                st=int(min(ziel_betrag,bar-3.0)//p)
                if st<=0 or st*p<minorder: continue
                g=gebuehr(st*p,modell)
                if st*p+g>bar: st-=1
                if st<=0 or st*p<minorder: continue
                bar-=st*p+g; geb+=g; orders+=1; pos[n]=st
        kurve.append(bar+sum(s*(heute[n] if not math.isnan(heute[n]) else 0) for n,s in pos.items()))
    k=pd.Series(kurve,index=idx[i0:i1])
    jahre=(k.index[-1]-k.index[0]).days/365.25
    cagr=(k.iloc[-1]/start)**(1/jahre)-1
    dd=(k/k.cummax()-1).min()
    return dict(cagr=cagr,dd=dd,geb_pa=geb/jahre/start,orders_pa=orders/jahre,kurve=k)
def gleich(c,von_i):
    px=c[[n for n in c.columns if n!='DAX']].iloc[von_i:]
    r=px.pct_change(fill_method=None).mean(axis=1).fillna(0)
    return (1+r).cumprod()
if __name__ == '__main__':
    c = lade(sys.argv[1]); R0 = 121
    g = gleich(c, R0)
    jahre_g = (g.index[-1] - g.index[0]).days / 365.25
    gj = g.groupby(g.index.year).agg(lambda s: s.iloc[-1] / s.iloc[0] - 1)
    print('Zeitraum', g.index[0].date(), g.index[-1].date(), round(jahre_g, 1), 'Jahre')
    print('Gleichgewicht brutto p.a. %.2f %%' % (100 * (g.iloc[-1] ** (1 / jahre_g) - 1)))
    print('N A H R | brutto 10000 | Papier 10000 | Fall B 1000 | Fall C 1000 | Orders/Jahr | Jahre brutto besser | Jahre B 1000 besser')
    def besser(k):
        kj = k.groupby(k.index.year).agg(lambda s: s.iloc[-1] / s.iloc[0] - 1)
        return int((kj > gj.reindex(kj.index)).sum()), len(kj)
    for (N, A) in [(2, 0.4), (4, 0.25)]:
        for R in [20, 120]:
            for H in [5, 10, 20, 60, 120]:
                rs = {(s, m): lauf(c.iloc[R0 - R - 1:], s, N, A, H, R, m)
                      for s, m in [(10000, 'null'), (10000, 'papier'), (1000, 'B'), (1000, 'C')]}
                a, b = rs[(10000, 'null')], rs[(1000, 'B')]
                print(N, A, H, R, '| %.2f | %.2f | %.2f | %.2f | %.0f | %d/%d | %d/%d' % (
                    100 * a['cagr'], 100 * rs[(10000, 'papier')]['cagr'], 100 * b['cagr'],
                    100 * rs[(1000, 'C')]['cagr'], a['orders_pa'], *besser(a['kurve']), *besser(b['kurve'])))
