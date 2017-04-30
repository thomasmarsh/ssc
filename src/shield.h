// --------------------------------------------------------------------------
//
// Copyright (c) 2003 Thomas D. Marsh. All rights reserved.
//
// "SSC" is free software; you can redistribute it
// and/or use it and/or modify it under the terms of
// the "GNU General Public License" (GPL).
//
// --------------------------------------------------------------------------

#ifndef SSC_SHIELD_H
#define SSC_SHIELD_H

#include "common.h"

class ShieldBase {
public:
    ShieldBase() {}
    virtual ~ShieldBase() {}

    virtual void setStrength(double val) {}
    virtual double getStrength() { return 0; }

    virtual void regenerate() {}
    virtual void damage(double amt) {}
};

class NoShieldPolicy : public ShieldBase {
};

const double SHIELD_RECHARGE_RATE = 7.5;

class ShieldPolicy {
public:
    ShieldPolicy()
        : strength(1)
        , counter(0)
    {
    }
    ~ShieldPolicy() {}

    void damage(double amt)
    {
        if (amt > strength) {
            strength = 0;
        } else {
            strength -= amt;
        }
        counter = 0;
    }

    inline void setStrength(double val) { strength = val; }

    inline double getStrength()
    {
        return strength;
    }

    void regenerate(double dt)
    {
        counter += dt;
        if (counter >= SHIELD_RECHARGE_RATE) {
            counter = 0;
            strength += .01;
            if (strength > 1) {
                strength = 1;
            }
        }
    }

private:
    double strength; // max 100
    double counter;
};

typedef ShieldPolicy Shield;

#endif // SSC_SHIELD_H
