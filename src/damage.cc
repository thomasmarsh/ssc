// --------------------------------------------------------------------------
//
// Copyright (c) 2003 Thomas D. Marsh. All rights reserved.
//
// "SSC" is free software; you can redistribute it
// and/or use it and/or modify it under the terms of
// the "GNU General Public License" (GPL).
//
// --------------------------------------------------------------------------

#include "bogey.h"
#include "ship.h"

bool Ship::collision(ScreenObject& obj)
{
    if (obj.ownerType() == PLAYER_TYPE) {
        return false;
    }

    switch (obj.type()) {
    case MISSILE_TYPE:
        damage(.10);
        break;
    case BOGEY_TYPE:
    case SMARTY_TYPE:
        //damage(.01);
        break;
    case LUNATIC_TYPE:
        damage(.005);
        return false;
        break;
    case ASTEROID_TYPE:
    case FATSO_TYPE:
        damage(.01);
        break;
    case BLACKHOLE_TYPE:
        damage(.10);
        break;
    case PLAYER_TYPE:
        break;
    }
    return true;
}

bool Bogey::collision(ScreenObject& obj)
{
    if (obj.mFlocking) {
        double d = distance(mPosition, obj.mPosition);
        if (d > 0 && d < 50) {
            interact(obj);
        }
    }

    switch (obj.type()) {
    case MISSILE_TYPE:
        if (obj.ownerType() != BOGEY_TYPE) {
            damage(.10);
        } else {
            return false;
        }
        break;
    case LUNATIC_TYPE:
        damage(.005);
        break;
    case ASTEROID_TYPE:
        break;
    case FATSO_TYPE:
        damage(.10);
        break;
    case BOGEY_TYPE:
        break;
    case BLACKHOLE_TYPE:
    case SMARTY_TYPE:
    case PLAYER_TYPE:
        damage(.01);
        break;
    }
    return true;
}
