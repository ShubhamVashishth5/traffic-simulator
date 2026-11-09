/**
 * A deck.gl layer that draws one oriented rectangle per vehicle (plan P4.3): an instanced unit
 * quad scaled to the vehicle's length × width and rotated by its heading, in world metres.
 * Data are binary attributes (typed arrays straight from the Arrow frame), so 20,000 vehicles
 * update without per-object work. Very small vehicles keep a minimum on-screen length so they
 * stay visible when zoomed out.
 */
import { Layer, picking, project32, type DefaultProps, type LayerProps, type UpdateParameters } from "@deck.gl/core";
import { Geometry, Model } from "@luma.gl/engine";

const uniformBlock = `\
layout(std140) uniform rectsUniforms {
  float sizeScale;
  float minPixels;
} rects;
`;

type RectsProps = { sizeScale: number; minPixels: number };

const rectsUniforms = {
  name: "rects",
  vs: uniformBlock,
  fs: uniformBlock,
  uniformTypes: { sizeScale: "f32", minPixels: "f32" },
} as const;

const vs = /* glsl */ `\
#version 300 es
#define SHADER_NAME oriented-rect-layer-vertex-shader

in vec3 positions;
in vec2 instancePositions;
in vec2 instanceSizes;
in float instanceAngles;
in vec4 instanceFillColors;

out vec4 vFillColor;

void main(void) {
  vec3 centre = vec3(instancePositions, 0.0);
  geometry.worldPosition = centre;
  geometry.pickingColor = picking_getPickingColorFromInstanceID();

  vec2 size = instanceSizes * rects.sizeScale;
  float lengthPx = project_size_to_pixel(size.x);
  float grow = lengthPx > 0.0 ? max(1.0, rects.minPixels / lengthPx) : 1.0;
  vec2 half_ = 0.5 * size * grow;
  vec2 local = positions.xy * half_;
  float c = cos(instanceAngles);
  float s = sin(instanceAngles);
  vec3 offset = vec3(project_size(vec2(local.x * c - local.y * s, local.x * s + local.y * c)), 0.0);
  DECKGL_FILTER_SIZE(offset, geometry);
  gl_Position = project_position_to_clipspace(centre, vec3(0.0), offset, geometry.position);
  DECKGL_FILTER_GL_POSITION(gl_Position, geometry);
  vFillColor = vec4(instanceFillColors.rgb, instanceFillColors.a * layer.opacity);
  DECKGL_FILTER_COLOR(vFillColor, geometry);
}
`;

const fs = /* glsl */ `\
#version 300 es
#define SHADER_NAME oriented-rect-layer-fragment-shader
precision highp float;

in vec4 vFillColor;
out vec4 fragColor;

void main(void) {
  fragColor = vFillColor;
  DECKGL_FILTER_COLOR(fragColor, geometry);
}
`;

/** Binary columns, `n` rows: positions and sizes have 2 values per row, colours 4 (RGBA bytes). */
export interface RectData {
  length: number;
  attributes: {
    getPosition: { value: Float32Array; size: 2 };
    getSize: { value: Float32Array; size: 2 };
    getAngle: { value: Float32Array; size: 1 };
    getFillColor: { value: Uint8Array; size: 4; normalized: true };
  };
}

export type OrientedRectLayerProps = LayerProps & {
  data: RectData;
  sizeScale?: number;
  minPixels?: number;
};

const defaultProps: DefaultProps<OrientedRectLayerProps> = {
  sizeScale: { type: "number", min: 0, value: 1 },
  minPixels: { type: "number", min: 0, value: 3 },
};

export class OrientedRectLayer extends Layer<Required<OrientedRectLayerProps>> {
  static layerName = "OrientedRectLayer";
  static defaultProps = defaultProps;

  declare state: { model?: Model };

  getShaders() {
    return super.getShaders({ vs, fs, modules: [project32, picking, rectsUniforms] });
  }

  initializeState() {
    this.getAttributeManager()!.addInstanced({
      instancePositions: { size: 2, type: "float32", accessor: "getPosition" },
      instanceSizes: { size: 2, type: "float32", accessor: "getSize", defaultValue: [4, 1.8] },
      instanceAngles: { size: 1, type: "float32", accessor: "getAngle" },
      instanceFillColors: { size: 4, type: "unorm8", accessor: "getFillColor", defaultValue: [255, 255, 255, 255] },
    });
  }

  updateState(params: UpdateParameters<this>) {
    super.updateState(params);
    if (params.changeFlags.extensionsChanged) {
      this.state.model?.destroy();
      this.state.model = this.getModel();
      this.getAttributeManager()!.invalidateAll();
    }
  }

  draw() {
    const model = this.state.model!;
    const props: RectsProps = { sizeScale: this.props.sizeScale, minPixels: this.props.minPixels };
    model.shaderInputs.setProps({ rects: props });
    model.draw(this.context.renderPass);
  }

  private getModel(): Model {
    return new Model(this.context.device, {
      ...this.getShaders(),
      id: this.props.id,
      bufferLayout: this.getAttributeManager()!.getBufferLayouts(),
      geometry: new Geometry({
        topology: "triangle-strip",
        attributes: { positions: { size: 3, value: new Float32Array([-1, -1, 0, 1, -1, 0, -1, 1, 0, 1, 1, 0]) } },
      }),
      isInstanced: true,
    });
  }
}
