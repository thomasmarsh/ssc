// --------------------------------------------------------------------------
//
// Copyright (c) 2003 Thomas D. Marsh. All rights reserved.
//
// "SSC" is free software; you can redistribute it
// and/or use it and/or modify it under the terms of
// the "GNU General Public License" (GPL).
//
// --------------------------------------------------------------------------

#ifndef SSC_MISSILE_H
#define SSC_MISSILE_H

#include "object.h"

class Missile : public ScreenObject {
private:
    Missile(Missile&);

public:
    double age;
    ObjectType owner_type;

    Missile(ObjectType t,
        double rotation,
        double x, double y, double z,
        double fx, double fy, double fz);
    ~Missile();

    ObjectType ownerType() { return owner_type; }
    void draw();
    void move(double dt);

    bool collision(ScreenObject& other);
};

class Bomb : public ScreenObject {
public:
    double mAge;
    ObjectType mOwnerType;

    Bomb(ObjectType t,
        double x, double y, double z,
        double fx, double fy, double fz);

    ~Bomb();

    void draw();
    void move(double dt);

    bool collision(ScreenObject& other);
};

#endif // SSC_MISSILE_H
