import { useEffect, useMemo, useRef, useState } from 'react';
import { IconArrowsDiagonal, IconDeviceFloppy, IconRefresh } from '@tabler/icons-react';
import { GlassButton } from './components/ui/glass-button';
import { GlassSlider } from './components/ui/glass-slider';
import type { AvatarRigSettings } from './frontend-types';

export type AvatarNaturalSize = { width: number; height: number };

type Props = {
  avatarSrc: string;
  rig: AvatarRigSettings;
  detached?: boolean;
  onRigChange?: (patch: Partial<AvatarRigSettings>) => void;
  onRigSave?: () => void;
  onPopout?: () => void;
  onSnap?: (size: AvatarNaturalSize) => void;
};

function clamp(value: number, min: number, max: number) {
  return Math.max(min, Math.min(max, value));
}

function RigSlider({
  label,
  value,
  min,
  max,
  step = 1,
  onChange
}: {
  label: string;
  value: number;
  min: number;
  max: number;
  step?: number;
  onChange: (value: number) => void;
}) {
  return (
    <div className="avatar-control-item">
      <div className="avatar-control-head">
        <span className="avatar-control-label">{label}</span>
        <span className="avatar-control-value">{Math.round(value)}</span>
      </div>
      <GlassSlider min={min} max={max} step={step} value={[value]} onValueChange={(values) => onChange(values[0] ?? value)} />
    </div>
  );
}

export function AvatarRuntime({ avatarSrc, rig, detached = false, onRigChange, onRigSave, onPopout, onSnap }: Props) {
  const [speaking, setSpeaking] = useState(false);
  const [controlsOpen, setControlsOpen] = useState(!detached);
  const [natural, setNatural] = useState<AvatarNaturalSize | null>(null);
  const snappedRef = useRef(false);

  useEffect(() => {
    const channel = typeof BroadcastChannel !== 'undefined' ? new BroadcastChannel('cohost-avatar-events') : null;
    if (channel) {
      channel.onmessage = (event) => {
        const type = event.data?.type;
        if (type === 'speak_start') setSpeaking(true);
        if (type === 'speak_stop') setSpeaking(false);
        if (type === 'snap_window' && detached) {
          // detached window resize is handled by parent Tauri window logic
        }
      };
    }
    return () => channel?.close();
  }, [detached]);

  const stageStyle = useMemo(
    () => ({
      ['--mouth-x' as string]: `${rig.mouthX}px`,
      ['--mouth-y' as string]: `${rig.mouthY}px`,
      ['--mouth-width' as string]: `${Math.max(42, rig.mouthWidth * 2)}px`,
      ['--mouth-open' as string]: `${clamp(rig.mouthOpen + (speaking ? 16 : 0), 0, 100)}`,
      ['--mouth-softness' as string]: `${rig.mouthSoftness}`,
      ['--mouth-smile' as string]: `${rig.mouthSmile}px`,
      ['--mouth-tilt' as string]: `${rig.mouthTilt}deg`,
      ['--mouth-color' as string]: rig.mouthColor,
      ['--brow-x' as string]: `${rig.browX}px`,
      ['--brow-y' as string]: `${rig.browY}px`,
      ['--brow-spacing' as string]: `${Math.max(42, rig.browSpacing * 2)}`,
      ['--brow-arch' as string]: `${rig.browArch}px`,
      ['--brow-tilt' as string]: `${rig.browTilt}deg`,
      ['--brow-thickness' as string]: `${rig.browThickness}px`,
      ['--brow-color' as string]: rig.browColor,
      ['--eye-open' as string]: `${rig.eyeOpen}`,
      ['--head-tilt' as string]: `${rig.headTilt}deg`,
      ['--head-scale' as string]: `${rig.headScale / 100}`,
      ['--glow' as string]: `${rig.glow / 100}`
    }),
    [rig, speaking]
  );

  const imageNode = (
    <>
      <img
        src={avatarSrc || '/floating-head.png'}
        alt="Avatar"
        className="avatar-photo"
        onLoad={(event) => {
          const width = event.currentTarget.naturalWidth || 320;
          const height = event.currentTarget.naturalHeight || 420;
          const next = { width, height };
          setNatural(next);
          if (!snappedRef.current) {
            snappedRef.current = true;
            onSnap?.(next);
          }
        }}
      />
      <div className="avatar-eye-mask avatar-eye-mask-left" />
      <div className="avatar-eye-mask avatar-eye-mask-right" />
      <div className="avatar-brow avatar-brow-left" />
      <div className="avatar-brow avatar-brow-right" />
      <div className="avatar-mouth-rig">
        <div className="avatar-mouth-outer">
          <div className="avatar-mouth-inner" />
          <div className="avatar-mouth-highlight" />
        </div>
      </div>
    </>
  );

  if (detached) {
    return (
      <div className="avatar-detached-root">
        <div className={`avatar-stage-panel detached-only ${speaking ? 'speaking' : ''}`} style={stageStyle}>
          <div className="avatar-stage-3d detached-scene">
            <div className="avatar-backlight" />
            <div className="avatar-face detached-face">{imageNode}</div>
            <div className="avatar-stage-shadow" />
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="avatar-shell embedded">
      <div className="avatar-shell-header">
        <div>
          <div className="section-title">Character Stage</div>
          <div className="panel-copy small-copy">
            Recommended portrait for this stage: 900×1400 minimum, 1200×1800 ideal. Keep the full head centered with extra forehead and chin room.
          </div>
        </div>
        <div className="avatar-shell-actions">
          {natural ? <div className="avatar-natural-size">{natural.width}×{natural.height}</div> : null}
          {onSnap ? <GlassButton size="sm" variant="default" onClick={() => natural && onSnap(natural)}><IconRefresh size={14} />Snap</GlassButton> : null}
          {onPopout ? <GlassButton size="sm" variant="default" onClick={onPopout}><IconArrowsDiagonal size={14} />Popup</GlassButton> : null}
          <GlassButton size="sm" variant="default" onClick={() => setControlsOpen((value) => !value)}>{controlsOpen ? 'Hide Rig' : 'Show Rig'}</GlassButton>
        </div>
      </div>

      <div className="avatar-stage-grid">
        <div className={`avatar-stage-panel ${speaking ? 'speaking' : ''}`} style={stageStyle}>
          <div className="avatar-stage-3d">
            <div className="avatar-backlight" />
            <div className="avatar-jar">
              <div className="avatar-glass-highlight" />
              <div className="avatar-glass-sheen" />
              <div className="avatar-face">{imageNode}</div>
              <div className="avatar-jar-rim avatar-jar-rim-top" />
              <div className="avatar-jar-rim avatar-jar-rim-bottom" />
            </div>
            <div className="avatar-stage-shadow" />
          </div>
        </div>

        {controlsOpen ? (
          <div className="avatar-rig-panel-react glass-inset">
            <div className="avatar-rig-toolbar">
              <div>
                <div className="section-title">Face Rig</div>
                {natural ? <div className="panel-copy small-copy">Current image {natural.width}×{natural.height}</div> : null}
              </div>
              {onRigSave ? <GlassButton size="sm" variant="primary" onClick={onRigSave}><IconDeviceFloppy size={14} />Save</GlassButton> : null}
            </div>
            <div className="avatar-rig-grid-react">
              <RigSlider label="Mouth X" value={rig.mouthX} min={-80} max={80} onChange={(value) => onRigChange?.({ mouthX: value })} />
              <RigSlider label="Mouth Y" value={rig.mouthY} min={-70} max={140} onChange={(value) => onRigChange?.({ mouthY: value })} />
              <RigSlider label="Mouth Width" value={rig.mouthWidth} min={16} max={46} onChange={(value) => onRigChange?.({ mouthWidth: value })} />
              <RigSlider label="Mouth Open" value={rig.mouthOpen} min={0} max={100} onChange={(value) => onRigChange?.({ mouthOpen: value })} />
              <RigSlider label="Mouth Smile" value={rig.mouthSmile} min={-40} max={40} onChange={(value) => onRigChange?.({ mouthSmile: value })} />
              <RigSlider label="Mouth Tilt" value={rig.mouthTilt} min={-25} max={25} onChange={(value) => onRigChange?.({ mouthTilt: value })} />
              <RigSlider label="Brow X" value={rig.browX} min={-60} max={60} onChange={(value) => onRigChange?.({ browX: value })} />
              <RigSlider label="Brow Y" value={rig.browY} min={-90} max={60} onChange={(value) => onRigChange?.({ browY: value })} />
              <RigSlider label="Brow Spacing" value={rig.browSpacing} min={20} max={46} onChange={(value) => onRigChange?.({ browSpacing: value })} />
              <RigSlider label="Brow Arch" value={rig.browArch} min={-30} max={30} onChange={(value) => onRigChange?.({ browArch: value })} />
              <RigSlider label="Brow Tilt" value={rig.browTilt} min={-25} max={25} onChange={(value) => onRigChange?.({ browTilt: value })} />
              <RigSlider label="Brow Thickness" value={rig.browThickness} min={2} max={24} onChange={(value) => onRigChange?.({ browThickness: value })} />
              <RigSlider label="Eye Open" value={rig.eyeOpen} min={10} max={100} onChange={(value) => onRigChange?.({ eyeOpen: value })} />
              <RigSlider label="Head Tilt" value={rig.headTilt} min={-20} max={20} onChange={(value) => onRigChange?.({ headTilt: value })} />
              <RigSlider label="Head Scale" value={rig.headScale} min={80} max={130} onChange={(value) => onRigChange?.({ headScale: value })} />
              <RigSlider label="Glow" value={rig.glow} min={0} max={100} onChange={(value) => onRigChange?.({ glow: value })} />
              <label className="glass-field color-field">
                <span className="glass-field-label">Lip tint</span>
                <input type="color" value={rig.mouthColor} onChange={(event) => onRigChange?.({ mouthColor: event.currentTarget.value })} className="glass-color-input" />
              </label>
              <label className="glass-field color-field">
                <span className="glass-field-label">Brow tint</span>
                <input type="color" value={rig.browColor} onChange={(event) => onRigChange?.({ browColor: event.currentTarget.value })} className="glass-color-input" />
              </label>
            </div>
          </div>
        ) : null}
      </div>
    </div>
  );
}
