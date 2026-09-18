"use strict";
(() => {
  const canvas=document.getElementById("brain-overlay");
  const frame=document.querySelector(".brain-frame");
  const stateEl=document.getElementById("brain-state");
  const rateEl=document.getElementById("event-rate");
  const densityEl=document.getElementById("signal-density");
  if(!canvas||!frame)return;
  const ctx=canvas.getContext("2d");
  const TAU=Math.PI*2;
  let w=1,h=1,dpr=1,t=0,lastState="";
  const palette={
    base:"rgba(155,183,199,.22)", gold:"rgba(200,169,107,.92)",
    bright:"rgba(224,200,138,.9)", cyan:"rgba(155,183,199,.92)",
    violet:"rgba(169,154,189,.9)", green:"rgba(169,199,160,.9)",
    red:"rgba(201,130,130,.92)", dim:"rgba(111,118,128,.28)"
  };
  const regions=[
    ["INPUT",.23,.28],["REASONING",.38,.44],["CORE",.50,.50],
    ["PLANNING",.65,.42],["ACTION",.78,.64],["VERIFY",.67,.78],
    ["OUTPUT",.50,.88],["MEMORY",.77,.25],["RECOVERY",.50,.16]
  ];
  const edges=[
    [0,1],[1,2],[2,3],[3,4],[4,5],[5,6],[5,2],[2,7],[7,1],[2,8]
  ];
  const stateMap={
    LISTENING:["INPUT",palette.cyan],UNDERSTANDING:["REASONING",palette.violet],
    THINKING:["REASONING",palette.gold],PLANNING:["PLANNING",palette.gold],
    SEARCHING:["MEMORY",palette.cyan],EXECUTING:["ACTION",palette.cyan],
    VERIFYING:["VERIFY",palette.green],COMPLETED:["OUTPUT",palette.green],
    WAITING:["INPUT",palette.gold],LEARNING:["MEMORY",palette.violet],
    WARNING:["VERIFY",palette.gold],ERROR:["RECOVERY",palette.red],
    RECOVERING:["RECOVERY",palette.violet],SECURITY_LOCK:["VERIFY",palette.red],
    RESOURCE_LIMITED:["CORE",palette.gold],SLEEPING:["CORE",palette.dim],
    OFFLINE:["CORE",palette.dim],IDLE:["CORE",palette.gold]
  };
  function resize(){
    const r=frame.getBoundingClientRect();
    dpr=Math.min(window.devicePixelRatio||1,2); w=Math.max(1,r.width); h=Math.max(1,r.height);
    canvas.width=Math.round(w*dpr);canvas.height=Math.round(h*dpr);
    canvas.style.width=w+"px";canvas.style.height=h+"px";
    ctx.setTransform(dpr,0,0,dpr,0,0);
  }
  function point(n){return [n[1]*w,n[2]*h]}
  function numeric(el,fallback){
    const v=parseFloat(String(el?.textContent||"").replace(/[^0-9.]/g,""));
    return Number.isFinite(v)?v:fallback;
  }
  function activeState(){return stateEl?.textContent?.trim().toUpperCase()||"OFFLINE"}
  function rgba(stroke,alpha){return stroke.replace(/rgba\\(([^)]+)\\)/,"rgba("+RegExp.$1.split(",").slice(0,3).join(",")+","+alpha+")")}
  function drawRing(cx,cy,r,start,end,width,stroke,alpha=1){
    ctx.beginPath();ctx.arc(cx,cy,r,start,end);ctx.lineWidth=width;
    ctx.strokeStyle=rgba(stroke,alpha);ctx.stroke();
  }
  function draw(){
    const state=activeState(), reduced=matchMedia("(prefers-reduced-motion: reduce)").matches;
    const [focus,focusColor]=stateMap[state]||stateMap.IDLE;
    const rate=Math.min(1,numeric(rateEl,0)/8);
    const density=Math.min(1,numeric(densityEl,0)/100);
    const activity=Math.max(.04,Math.min(1,rate*.7+density*.3));
    const motion=reduced?0:1;
    t+=.012*motion*(.35+activity);
    ctx.clearRect(0,0,w,h);
    const cx=w*.5,cy=h*.50,base=Math.min(w,h)*.19;
    const breath=1+Math.sin(t*.7)*.006*(state==="IDLE"||state==="SLEEPING"?1:.25);
    ctx.save();
    ctx.globalCompositeOperation="lighter";
    for(let i=0;i<4;i++){
      const r=base*(1.35+i*.27);
      const speed=(i%2?-1:1)*(.025+i*.006)*(state==="IDLE"?.35:1);
      drawRing(cx,cy,r,t*speed+i*1.4,t*speed+i*1.4+Math.PI*(i===0?.7:.25),1,
        i===1?palette.base:palette.gold,.18+(activity*.12));
    }
    ctx.restore();
    ctx.save();
    ctx.globalCompositeOperation="source-over";
    edges.forEach(([a,b])=>{
      const p=point(regions[a]),q=point(regions[b]);
      ctx.beginPath();ctx.moveTo(p[0],p[1]);ctx.lineTo(q[0],q[1]);
      ctx.lineWidth=regions[a][0]===focus||regions[b][0]===focus?1.35:1;
      ctx.strokeStyle=regions[a][0]===focus||regions[b][0]===focus?focusColor:palette.base;
      ctx.globalAlpha=regions[a][0]===focus||regions[b][0]===focus?.7:.32;
      ctx.stroke();
    });
    ctx.globalAlpha=1;
    regions.forEach((n,i)=>{
      const [x,y]=point(n),active=n[0]===focus||n[0]==="CORE";
      const r=(n[0]==="CORE"?5.5:3.2)*(active?1.2:1);
      ctx.beginPath();ctx.arc(x,y,r,0,TAU);
      ctx.fillStyle=active?focusColor:palette.dim;ctx.fill();
      if(active){
        ctx.beginPath();ctx.arc(x,y,r+5+Math.sin(t*2+i)*1.5,0,TAU);
        ctx.strokeStyle=focusColor;ctx.globalAlpha=.18+.08*activity;ctx.stroke();ctx.globalAlpha=1;
      }
    });
    const coreR=base*.42*breath;
    const grad=ctx.createRadialGradient(cx,cy,0,cx,cy,coreR*2.8);
    grad.addColorStop(0,rgba(focusColor,.18));
    grad.addColorStop(1,"rgba(0,0,0,0)");
    ctx.fillStyle=grad;ctx.beginPath();ctx.arc(cx,cy,coreR*2.8,0,TAU);ctx.fill();
    ctx.beginPath();ctx.arc(cx,cy,coreR,0,TAU);
    ctx.strokeStyle=focusColor;ctx.globalAlpha=.42+.18*activity;ctx.lineWidth=1;ctx.stroke();
    ctx.globalAlpha=1;
    if(motion && state!=="IDLE" && state!=="SLEEPING" && state!=="OFFLINE"){
      const route={INPUT:[0,1],REASONING:[1,2],PLANNING:[2,3],ACTION:[3,4],VERIFY:[4,5],OUTPUT:[2,6],MEMORY:[2,7],RECOVERY:[2,8],CORE:[2,6]}[focus]||[2,6];
      for(let k=0;k<Math.max(1,Math.ceil(1+activity*3));k++){
        const phase=(t*(.65+activity*.9)+k*.31)%1;
        const a=point(regions[route[0]]),b=point(regions[route[1]]);
        const x=a[0]+(b[0]-a[0])*phase,y=a[1]+(b[1]-a[1])*phase;
        ctx.beginPath();ctx.arc(x,y,1.5+activity*1.2,0,TAU);ctx.fillStyle=focusColor;ctx.globalAlpha=.75;ctx.fill();
      }
      ctx.globalAlpha=1;
    }
    ctx.restore();
    lastState=state;
    requestAnimationFrame(draw);
  }
  new MutationObserver(()=>{if(activeState()!==lastState){resize()}}).observe(document.documentElement,{attributes:true,attributeFilter:["data-state"]});
  window.addEventListener("resize",resize);
  resize();draw();
})();