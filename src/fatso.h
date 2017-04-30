// --------------------------------------------------------------------------
//
// Copyright (c) 2003 Thomas D. Marsh. All rights reserved.
//
// "SSC" is free software; you can redistribute it
// and/or use it and/or modify it under the terms of
// the "GNU General Public License" (GPL).
//
// --------------------------------------------------------------------------

#ifndef SSC_FATSO_H
#define SSC_FATSO_H

#include "object.h"

const unsigned int FATSO_SPEED = 7;
const unsigned int FATSO_RADIUS = 50;
const double FATSO_MASS = 200.0;

class Fatso : public ScreenObject {
private:
    Fatso(Fatso&);

public:
    Fatso();
    ~Fatso();

    void draw();
    void move(double dt) { ScreenObject::move(dt); }
    bool collision(ScreenObject&) { return true; }
};

#endif // SSC_FATSO_H
