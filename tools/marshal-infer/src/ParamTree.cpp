// ParamTree.cpp — see ParamTree.h. DWARF (DIType) -> parameter tree.
#include "ParamTree.h"

#include "llvm/BinaryFormat/Dwarf.h"
#include "llvm/IR/DebugInfoMetadata.h"

using namespace llvm;

namespace marshal {

const char *nodeKindName(NodeKind k) {
  switch (k) {
  case NodeKind::Scalar:  return "scalar";
  case NodeKind::Pointer: return "ptr";
  case NodeKind::Struct:  return "struct";
  case NodeKind::Union:   return "union";
  case NodeKind::Array:   return "array";
  case NodeKind::Unknown: return "unknown";
  }
  return "unknown";
}

const char *dirName(Dir d) {
  switch (d) {
  case Dir::NA:      return "na";
  case Dir::In:      return "in";
  case Dir::Out:     return "out";
  case Dir::InOut:   return "inout";
  case Dir::Unknown: return "unknown";
  }
  return "unknown";
}

const char *sizeKindName(SizeKind s) {
  switch (s) {
  case SizeKind::NA:             return "na";
  case SizeKind::Const:          return "const";
  case SizeKind::FromArg:        return "from_arg";
  case SizeKind::FromArgPointee: return "from_arg_pointee";
  case SizeKind::Cstr:           return "cstr";
  case SizeKind::PtrArray:       return "ptr_array";
  case SizeKind::Unknown:        return "unknown";
  }
  return "unknown";
}

const char *retKindName(RetKind r) {
  switch (r) {
  case RetKind::Void:        return "void";
  case RetKind::Scalar:      return "scalar";
  case RetKind::PtrAliasArg: return "ptr_alias_arg";
  case RetKind::PtrIntoArg:  return "ptr_into_arg";
  case RetKind::PtrIntoCursor: return "ptr_into_cursor";
  case RetKind::PtrToStatic: return "ptr_to_static";
  case RetKind::PtrAlloc:    return "ptr_alloc";
  case RetKind::Handle:      return "handle";
  case RetKind::ForceLocal:  return "force_local";
  }
  return "void";
}

// Peel typedef / const / volatile / restrict / atomic wrappers to reach the
// type that actually determines layout & marshalling. (KSplit: stripMemberTag /
// stripAttributes, Tree.cpp:36.)
static const DIType *stripType(const DIType *ty) {
  while (auto *dt = dyn_cast_or_null<DIDerivedType>(ty)) {
    switch (dt->getTag()) {
    case dwarf::DW_TAG_typedef:
    case dwarf::DW_TAG_const_type:
    case dwarf::DW_TAG_volatile_type:
    case dwarf::DW_TAG_restrict_type:
    case dwarf::DW_TAG_atomic_type:
      ty = dt->getBaseType();
      continue;
    default:
      return ty;
    }
  }
  return ty;
}

// Does the type chain carry a `const` qualifier (before stripping)?
static bool hasConstQual(const DIType *ty) {
  while (auto *dt = dyn_cast_or_null<DIDerivedType>(ty)) {
    switch (dt->getTag()) {
    case dwarf::DW_TAG_const_type:
      return true;
    case dwarf::DW_TAG_typedef:
    case dwarf::DW_TAG_volatile_type:
    case dwarf::DW_TAG_restrict_type:
    case dwarf::DW_TAG_atomic_type:
      ty = dt->getBaseType();
      continue;
    default:
      return false;
    }
  }
  return false;
}

static std::string typeNameOf(const DIType *ty) {
  if (!ty)
    return "void";
  if (!ty->getName().empty())
    return ty->getName().str();
  // Anonymous composite / derived: synthesize a readable tag.
  switch (ty->getTag()) {
  case dwarf::DW_TAG_structure_type: return "<anon struct>";
  case dwarf::DW_TAG_union_type:     return "<anon union>";
  case dwarf::DW_TAG_pointer_type:   return "<ptr>";
  case dwarf::DW_TAG_array_type:     return "<array>";
  default:                           return "<anon>";
  }
}

std::unique_ptr<TreeNode> buildTreeFromDIType(const DIType *rawTy,
                                              unsigned maxDepth) {
  auto node = std::make_unique<TreeNode>();
  const DIType *ty = stripType(rawTy);

  node->typeName = typeNameOf(ty);
  node->sizeBytes = ty ? ty->getSizeInBits() / 8 : 0;

  if (!ty) {
    node->kind = NodeKind::Unknown; // void
    return node;
  }

  // Depth guard (cycle / blowup): stop unfolding, mark residue.
  if (maxDepth == 0) {
    node->kind = NodeKind::Unknown;
    node->depthTruncated = true;
    return node;
  }

  const unsigned tag = ty->getTag();

  if (tag == dwarf::DW_TAG_pointer_type) {
    node->kind = NodeKind::Pointer;
    const DIType *rawPointee = cast<DIDerivedType>(ty)->getBaseType();
    node->pointeeConst = hasConstQual(rawPointee);
    const DIType *pointee = stripType(rawPointee);
    if (!pointee) {
      node->pointeeOpaque = true; // void* — residue
    } else {
      node->children.push_back(buildTreeFromDIType(pointee, maxDepth - 1));
    }
    return node;
  }

  if (tag == dwarf::DW_TAG_structure_type ||
      tag == dwarf::DW_TAG_union_type) {
    node->kind = (tag == dwarf::DW_TAG_union_type) ? NodeKind::Union
                                                   : NodeKind::Struct;
    auto *comp = cast<DICompositeType>(ty);
    for (DINode *elt : comp->getElements()) {
      auto *member = dyn_cast_or_null<DIDerivedType>(elt);
      if (!member || member->getTag() != dwarf::DW_TAG_member)
        continue; // skip methods / static members / inheritance entries
      auto child = buildTreeFromDIType(member->getBaseType(), maxDepth - 1);
      child->fieldName = member->getName().str();
      child->offsetBytes = member->getOffsetInBits() / 8;
      node->children.push_back(std::move(child));
    }
    return node;
  }

  if (tag == dwarf::DW_TAG_array_type) {
    node->kind = NodeKind::Array;
    auto *comp = cast<DICompositeType>(ty);
    node->children.push_back(
        buildTreeFromDIType(comp->getBaseType(), maxDepth - 1));
    return node;
  }

  if (tag == dwarf::DW_TAG_subroutine_type) {
    // A function type (reached via a function pointer). Not marshalable as data;
    // mark Unknown so a struct carrying a vtable becomes unresolvable.
    node->kind = NodeKind::Unknown;
    node->typeName = "<func>";
    return node;
  }

  // DW_TAG_base_type, DW_TAG_enumeration_type, etc. -> scalar leaf.
  node->kind = NodeKind::Scalar;
  return node;
}

std::unique_ptr<FunctionTrees> buildFunctionTrees(const DISubprogram *sp,
                                                  unsigned maxDepth) {
  if (!sp)
    return nullptr;
  auto *subTy = sp->getType();
  if (!subTy)
    return nullptr;
  DITypeRefArray typeArray = subTy->getTypeArray();
  if (typeArray.size() == 0)
    return nullptr;

  auto ft = std::make_unique<FunctionTrees>();
  ft->funcName = sp->getName().str();

  // Index 0 is the return type (null = void); 1.. are the parameters.
  if (DIType *retTy = typeArray[0])
    ft->ret = buildTreeFromDIType(retTy, maxDepth);

  for (unsigned i = 1; i < typeArray.size(); ++i) {
    DIType *argTy = typeArray[i];
    // A null entry in the middle would be a varargs sentinel; stop there.
    if (!argTy)
      break;
    ft->params.push_back(buildTreeFromDIType(argTy, maxDepth));
  }
  return ft;
}

} // namespace marshal
