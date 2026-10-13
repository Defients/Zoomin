export interface Point { x: number; y: number }
export interface Rect { left: number; top: number; right: number; bottom: number }
export interface Settings {
 version:number; zoom:number; presets:[number,number,number]; sensitivity:number;
 tracking:'instant'|'smooth'|'fixed'; smoothingMs:number; placement:'follow'|'pinned'|'docked'; offset:Point;
 width:number; height:number; border:number; accent:string; clickThrough:boolean; dockEdge:'top'|'bottom'|'left'|'right'; dockSize:number;
 shortcuts:{action:string;chord:string}[]; crosshair:boolean; grid:boolean; gridThreshold:number; pixelPerfect:boolean;
 streamProfile:'visible'|'local'|'minimal'; startup:boolean; onboardingDone:boolean;
}
export interface Snapshot {
 initialized:boolean;visible:boolean;holding:boolean;pinned:boolean;displayedZoom:number;cursor:Point;target:Point;source:Rect;lens:Rect;monitor:Rect;dpi:number;
 updateHz:number;updateMs:number;sample:null|{hex:string;rgb:string;hsl:string;r:number;g:number;b:number};error:string|null;settings:Settings;backend:string;inspectorActive:boolean;
 measuring:string|null;measurement:null|{kind:string;start:Point;end:Point;width:number;height:number;distance:number};
}
