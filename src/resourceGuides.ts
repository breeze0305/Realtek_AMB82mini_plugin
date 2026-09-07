import boxClassList from "./assets/resource-guides/box/box-class-list.png";
import classificationClassList from "./assets/resource-guides/classification/classification-class-list.png";
import classificationClassListTab from "./assets/resource-guides/classification/classification-class-list-tab.png";
import classificationModelSelection from "./assets/resource-guides/classification/model-selection.png";
import classificationWeightLocation from "./assets/resource-guides/classification/weight-folder-location.png";
import carWiringDiagram from "./assets/resource-guides/gesture/car-wiring-diagram.png";
import gestureClassList from "./assets/resource-guides/gesture/gesture-class-list.png";
import gestureModelSelection from "./assets/resource-guides/gesture/model-selection.png";
import gestureClassListTab from "./assets/resource-guides/gesture/object-class-list-tab.png";
import gestureOpenFolder from "./assets/resource-guides/gesture/open-amebapro2-folder.png";
import gestureWeightLocation from "./assets/resource-guides/gesture/weight-folder-location.png";

export type ResourceGuide = {
  isPlaceholder: boolean;
  summary: string;
  sections: Array<{
    title: string;
    body: string;
    codeExamples?: Array<{ label: string; code: string }>;
    image?: { src?: string; alt: string; caption?: string };
    link?: { label: string; url: string };
  }>;
};

type ResourceGuideDefinition = {
  isPlaceholder?: boolean;
  nameKey: string;
  summaryKey: string;
  sections: Array<{
    titleKey: string;
    bodyKey: string;
    codeExamples?: Array<{ labelKey: string; code: string }>;
    image?: { src?: string; altKey: string; captionKey?: string };
    link?: { labelKey: string; url: string };
  }>;
};

// Tracking resources share the setup steps, wiring diagram, and assembly video.
// Keep their class settings and resource-specific wording separate.
function createTrackingSections(variant: "Hand" | "Box", classListImage: string): ResourceGuideDefinition["sections"] {
  return [
    {
      titleKey: "resourceHandCodeTitle",
      bodyKey: "resourceHandCodeGuide",
      codeExamples: [
        {
          labelKey: "resourceGuideBeforeCode",
          code: "ObjDet.modelSelect(OBJECT_DETECTION, DEFAULT_YOLOV4TINY, NA_MODEL, NA_MODEL);",
        },
        {
          labelKey: "resourceGuideAfterCode",
          code: "ObjDet.modelSelect(OBJECT_DETECTION, CUSTOMIZED_YOLOV7TINY, NA_MODEL, NA_MODEL);",
        },
      ],
      image: {
        src: gestureModelSelection,
        altKey: "resourceHandModelImageAlt",
        captionKey: "resourceHandModelImageCaption",
      },
    },
    {
      titleKey: "resourceHandClassTabTitle",
      bodyKey: "resourceHandClassTabBody",
      image: {
        src: gestureClassListTab,
        altKey: "resourceHandTabImageAlt",
        captionKey: "resourceHandTabImageCaption",
      },
    },
    {
      titleKey: `resource${variant}ClassesTitle`,
      bodyKey: `resource${variant}ClassesBody`,
      image: {
        src: classListImage,
        altKey: `resource${variant}ClassesImageAlt`,
        captionKey: `resource${variant}ClassesImageCaption`,
      },
    },
    {
      titleKey: "resourceHandOpenFolderTitle",
      bodyKey: "resourceHandOpenFolderBody",
      image: {
        src: gestureOpenFolder,
        altKey: "resourceHandOpenFolderImageAlt",
        captionKey: "resourceHandOpenFolderImageCaption",
      },
    },
    {
      titleKey: `resource${variant}WeightLocationTitle`,
      bodyKey: "resourceHandWeightLocationBody",
      image: {
        src: gestureWeightLocation,
        altKey: "resourceHandWeightLocationImageAlt",
        captionKey: "resourceHandWeightLocationImageCaption",
      },
    },
    {
      titleKey: "resourceHandCarTitle",
      bodyKey: `resource${variant}CarBody`,
    },
    {
      titleKey: "resourceHandWiringTitle",
      bodyKey: "resourceHandWiringBody",
      image: {
        src: carWiringDiagram,
        altKey: "resourceHandWiringImageAlt",
        captionKey: "resourceHandWiringImageCaption",
      },
    },
    {
      titleKey: "resourceHandAssemblyTitle",
      bodyKey: "resourceHandAssemblyBody",
      link: {
        labelKey: "resourceHandAssemblyLink",
        url: "https://www.youtube.com/watch?v=UpYyOiEFA0k",
      },
    },
  ];
}

// Classification weights differ in training content, but use the same setup and class names.
const classificationSections: ResourceGuideDefinition["sections"] = [
  {
    titleKey: "resourceHandCodeTitle",
    bodyKey: "resourceClassificationCodeGuide",
    codeExamples: [
      {
        labelKey: "resourceGuideBeforeCode",
        code: "imgclass.modelSelect(IMAGE_CLASSIFICATION, NA_MODEL, NA_MODEL, NA_MODEL, NA_MODEL, DEFAULT_IMGCLASS);",
      },
      {
        labelKey: "resourceGuideAfterCode",
        code: "imgclass.modelSelect(IMAGE_CLASSIFICATION, NA_MODEL, NA_MODEL, NA_MODEL, NA_MODEL, CUSTOMIZED_IMGCLASS);",
      },
    ],
    image: {
      src: classificationModelSelection,
      altKey: "resourceClassificationModelImageAlt",
      captionKey: "resourceClassificationModelImageCaption",
    },
  },
  {
    titleKey: "resourceHandClassTabTitle",
    bodyKey: "resourceClassificationClassTabBody",
    image: {
      src: classificationClassListTab,
      altKey: "resourceClassificationTabImageAlt",
      captionKey: "resourceClassificationTabImageCaption",
    },
  },
  {
    titleKey: "resourceClassificationClassesTitle",
    bodyKey: "resourceClassificationClassesBody",
    image: {
      src: classificationClassList,
      altKey: "resourceClassificationClassesImageAlt",
      captionKey: "resourceClassificationClassesImageCaption",
    },
  },
  {
    titleKey: "resourceHandOpenFolderTitle",
    bodyKey: "resourceHandOpenFolderBody",
    image: {
      src: gestureOpenFolder,
      altKey: "resourceHandOpenFolderImageAlt",
      captionKey: "resourceHandOpenFolderImageCaption",
    },
  },
  {
    titleKey: "resourceClassificationWeightLocationTitle",
    bodyKey: "resourceClassificationWeightLocationBody",
    image: {
      src: classificationWeightLocation,
      altKey: "resourceClassificationWeightLocationImageAlt",
      captionKey: "resourceClassificationWeightLocationImageCaption",
    },
  },
];

// Each resource owns its content keys and placeholder status.
// Set isPlaceholder to false when replacing a resource's placeholder guide.
const resourceGuides: Record<string, ResourceGuideDefinition> = {
  "resource-hand": {
    isPlaceholder: false,
    nameKey: "hand",
    summaryKey: "resourceHandGuideSummary",
    sections: createTrackingSections("Hand", gestureClassList),
  },
  "resource-box": {
    isPlaceholder: false,
    nameKey: "objectBoxTracking",
    summaryKey: "resourceBoxGuideSummary",
    sections: createTrackingSections("Box", boxClassList),
  },
  "resource-japan": {
    isPlaceholder: false,
    nameKey: "japanModel",
    summaryKey: "resourceJapanGuideSummary",
    sections: classificationSections,
  },
  "resource-taiwan": {
    isPlaceholder: false,
    nameKey: "taiwanModel",
    summaryKey: "resourceTaiwanGuideSummary",
    sections: classificationSections,
  },
  "resource-singapore": {
    isPlaceholder: false,
    nameKey: "singaporeModel",
    summaryKey: "resourceSingaporeGuideSummary",
    sections: classificationSections,
  },
};

export function getResourceGuide(resourceId: string, t: Record<string, string>): ResourceGuide {
  const placeholder = t.resourceGuidePlaceholder ?? "Guide content coming soon";
  const definition = Object.prototype.hasOwnProperty.call(resourceGuides, resourceId)
    ? resourceGuides[resourceId]
    : undefined;

  if (!definition) {
    return {
      isPlaceholder: true,
      summary: placeholder,
      sections: [{ title: t.resourceGuideTitle ?? "Usage guide", body: placeholder }],
    };
  }

  const name = t[definition.nameKey] ?? "";
  const translate = (key: string) => (t[key] ?? placeholder).split("{resource}").join(name);

  return {
    isPlaceholder: definition.isPlaceholder ?? true,
    summary: translate(definition.summaryKey),
    sections: definition.sections.map((section) => ({
      title: translate(section.titleKey),
      body: translate(section.bodyKey),
      ...(section.link ? { link: { label: translate(section.link.labelKey), url: section.link.url } } : {}),
      ...(section.codeExamples
        ? {
            codeExamples: section.codeExamples.map((example) => ({
              label: translate(example.labelKey),
              code: example.code,
            })),
          }
        : {}),
      ...(section.image
        ? {
            image: {
              src: section.image.src,
              alt: translate(section.image.altKey),
              caption: section.image.captionKey ? translate(section.image.captionKey) : undefined,
            },
          }
        : {}),
    })),
  };
}
