/**
 * A video of the viewer (extras D17, D19): the webview's own controls
 * (keyboard, captions), never autoplaying; only the metadata loads until
 * the user plays it, and the asset protocol answers `Range` requests so
 * it seeks without reading the whole file. A media error gives up.
 */
export function VideoView({
  url,
  name,
  onUnplayable,
}: {
  url: string;
  name: string;
  onUnplayable(): void;
}) {
  return (
    <div className="media-viewer__stage media-viewer__stage--video">
      <div className="media-viewer__canvas">
        {/* eslint-disable-next-line jsx-a11y/media-has-caption -- a user's file: its own caption tracks show in the native controls; there is no sidecar to add */}
        <video
          className="media-viewer__video"
          src={url}
          aria-label={name}
          controls
          preload="metadata"
          playsInline
          onError={onUnplayable}
        />
      </div>
    </div>
  );
}
