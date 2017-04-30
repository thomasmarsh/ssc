// --------------------------------------------------------------------------
//
// Copyright (c) 2003 Thomas D. Marsh. All rights reserved.
//
// "SSC" is free software; you can redistribute it
// and/or use it and/or modify it under the terms of
// the "GNU General Public License" (GPL).
//
// --------------------------------------------------------------------------

#ifndef SSC_LEVEL_H
#define SSC_LEVEL_H

#include "model.h"

class Level
{
public:
        Level();
        virtual ~Level();

        void setLevel(int);

        void operator ++ (int);

        bool completed();
private:
};


#endif // SSC_LEVEL_H
