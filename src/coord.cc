#include "coord.h"

template <>
const Coord2<double> Coord2<double>::zero(0, 0);
template <>
const Coord3<double> Coord3<double>::zero(0, 0, 0);
template <>
const Coord3<double> Coord3<double>::up(0, 1, 0);
template <>
const Coord3<double> Coord3<double>::forward(0, 0, 1);
template <>
const Coord3<double> Coord3<double>::side(-1, 0, 0);
